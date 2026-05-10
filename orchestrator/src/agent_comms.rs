//! Reusable agent connection and messaging primitives.
//!
//! Extracts the OpenClaw WebSocket connect + Ed25519 handshake so it can be
//! used from chat proxy, agent-to-agent send, team chat, and websocket proxy.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message as TungsteniteMessage},
    MaybeTlsStream, WebSocketStream,
};

use crate::api::{build_device_connect_request, load_or_create_device_keys};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Authenticated, split WebSocket streams to an agent's OpenClaw gateway.
pub struct AgentConnection {
    pub tx: futures_util::stream::SplitSink<WsStream, TungsteniteMessage>,
    pub rx: futures_util::stream::SplitStream<WsStream>,
}

/// Connect to an agent's OpenClaw gateway and complete the Ed25519 handshake.
///
/// Returns authenticated split (tx, rx) streams ready for chat.send or other
/// OpenClaw RPC methods.
pub async fn connect_to_agent(gateway_port: u16) -> Result<AgentConnection> {
    connect_to_agent_with_token(gateway_port, None).await
}

/// Connect to an agent's OpenClaw gateway with an explicit gateway token.
pub async fn connect_to_agent_with_token(
    gateway_port: u16,
    gateway_token: Option<&str>,
) -> Result<AgentConnection> {
    let agent_ws_url = format!("ws://127.0.0.1:{}", gateway_port);
    tracing::info!("Connecting to agent at {}", agent_ws_url);

    let config = WebSocketConfig {
        accept_unmasked_frames: true,
        ..Default::default()
    };

    // Retry connection with exponential backoff
    let mut ws_stream = None;
    let mut last_error = String::from("Unknown error");

    for retry in 0..5 {
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            connect_async_with_config(&agent_ws_url, Some(config), false),
        )
        .await
        {
            Ok(Ok(stream)) => {
                tracing::info!(
                    "Connected to agent websocket at {} (attempt {})",
                    agent_ws_url,
                    retry + 1
                );
                ws_stream = Some(stream);
                break;
            }
            Ok(Err(e)) => {
                last_error = e.to_string();
                if retry < 4 {
                    let delay = 100 * 2_u64.pow(retry as u32);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
            }
            Err(_) => {
                last_error = "timeout".to_string();
                if retry < 4 {
                    let delay = 100 * 2_u64.pow(retry as u32);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }

    let (ws, _) = ws_stream.ok_or_else(|| {
        anyhow!(
            "Failed to connect to agent at {} after 5 retries: {}",
            agent_ws_url,
            last_error
        )
    })?;

    let (mut tx, mut rx) = ws.split();

    // Load device keys
    let (signing_key, device_id) = load_or_create_device_keys()?;
    let device_token = {
        use base64::Engine;
        let token_input = format!("openclaw-device-token:{}", device_id);
        let token_hash = <sha2::Sha256 as sha2::Digest>::digest(token_input.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_hash)
    };

    // Wait for connect.challenge
    let nonce = match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        rx.next(),
    )
    .await
    {
        Ok(Some(Ok(TungsteniteMessage::Text(text)))) => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|j| j["payload"]["nonce"].as_str().map(String::from))
            .ok_or_else(|| anyhow!("No nonce in challenge message"))?,
        Ok(Some(Ok(TungsteniteMessage::Close(frame)))) => {
            let reason = frame
                .as_ref()
                .map(|f| format!("code={}, reason={}", f.code, f.reason))
                .unwrap_or_default();
            return Err(anyhow!("Agent closed during handshake: {}", reason));
        }
        _ => return Err(anyhow!("No challenge received from agent (timeout)")),
    };

    // Send signed connect request
    let connect_id = uuid::Uuid::new_v4().to_string();
    let connect_request = build_device_connect_request(
        &connect_id,
        &nonce,
        &signing_key,
        &device_id,
        gateway_token,
        Some(&device_token),
    );

    tx.send(TungsteniteMessage::Text(connect_request.to_string()))
        .await
        .map_err(|e| anyhow!("Failed to send connect request: {}", e))?;

    // Wait for connect response
    let mut authenticated = false;
    for _ in 0..10 {
        match tokio::time::timeout(tokio::time::Duration::from_secs(15), rx.next()).await {
            Ok(Some(Ok(TungsteniteMessage::Text(resp)))) => {
                if let Ok(rj) = serde_json::from_str::<serde_json::Value>(&resp) {
                    if rj["type"] == "res" && rj["ok"] == true {
                        authenticated = true;
                        break;
                    } else if rj["type"] == "res" && rj["ok"] == false {
                        let err_msg = rj["error"]["message"]
                            .as_str()
                            .unwrap_or("unknown error");
                        return Err(anyhow!("Connect rejected: {}", err_msg));
                    }
                    // Other events (pairing notifications) — keep waiting
                }
            }
            Ok(Some(Ok(TungsteniteMessage::Close(frame)))) => {
                let reason = frame
                    .as_ref()
                    .map(|f| format!("code={}, reason={}", f.code, f.reason))
                    .unwrap_or_default();
                return Err(anyhow!("Agent closed after connect: {}", reason));
            }
            _ => break,
        }
    }

    if !authenticated {
        tracing::warn!("Device auth not confirmed, proceeding anyway");
    }

    Ok(AgentConnection { tx, rx })
}

/// Send a single message to an agent and wait for the complete response.
///
/// Opens a fresh WebSocket, authenticates, sends a `chat.send`, collects
/// text chunks until the `state: "final"` event, then returns the full text.
pub async fn send_message_to_agent(
    gateway_port: u16,
    gateway_token: Option<&str>,
    message: &str,
    timeout_secs: u64,
) -> Result<String> {
    let conn = connect_to_agent_with_token(gateway_port, gateway_token).await?;
    let AgentConnection { mut tx, mut rx } = conn;

    // Send chat.send request
    let request_id = uuid::Uuid::new_v4().to_string();
    let idempotency_key = format!("idem-{}", uuid::Uuid::new_v4());
    let chat_request = serde_json::json!({
        "type": "req",
        "id": request_id,
        "method": "chat.send",
        "params": {
            "sessionKey": "agent:dev:main",
            "message": {
                "role": "user",
                "content": [
                    { "type": "text", "text": message }
                ]
            },
            "idempotencyKey": idempotency_key
        }
    });

    tx.send(TungsteniteMessage::Text(chat_request.to_string()))
        .await
        .map_err(|e| anyhow!("Failed to send chat message: {}", e))?;

    // Collect response text until final event
    let mut response_text = String::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            if response_text.is_empty() {
                return Err(anyhow!("Timeout waiting for agent response"));
            }
            break;
        }

        match tokio::time::timeout(remaining, rx.next()).await {
            Ok(Some(Ok(TungsteniteMessage::Text(text)))) => {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Check for chat event with text content
                    if event.get("event").and_then(|e| e.as_str()) == Some("chat") {
                        if let Some(payload) = event.get("payload") {
                            let is_final =
                                payload.get("state").and_then(|s| s.as_str()) == Some("final");

                            // Extract text from content array
                            if let Some(content_arr) = payload
                                .get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_array())
                            {
                                for item in content_arr {
                                    if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        if let Some(t) = item.get("text").and_then(|t| t.as_str())
                                        {
                                            response_text.push_str(t);
                                        }
                                    }
                                }
                            }

                            if is_final {
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Some(Ok(TungsteniteMessage::Close(_)))) => break,
            Ok(Some(Ok(_))) => {} // Binary, Ping, Pong, Frame — ignore
            Ok(Some(Err(e))) => return Err(anyhow!("WebSocket error: {}", e)),
            Ok(None) => break,
            Err(_) => {
                if response_text.is_empty() {
                    return Err(anyhow!("Timeout waiting for agent response"));
                }
                break;
            }
        }
    }

    // Close gracefully
    let _ = tx.send(TungsteniteMessage::Close(None)).await;

    Ok(response_text)
}
