use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message as TungsteniteMessage},
    MaybeTlsStream, WebSocketStream,
};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct AgentConnection {
    pub tx: futures_util::stream::SplitSink<WsStream, TungsteniteMessage>,
    pub rx: futures_util::stream::SplitStream<WsStream>,
}

pub async fn connect_to_agent_with_token(
    gateway_port: u16,
    gateway_token: Option<&str>,
) -> Result<AgentConnection> {
    let agent_ws_url = format!("ws://127.0.0.1:{}", gateway_port);
    println!("Connecting to agent at {}", agent_ws_url);

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
                println!(
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
        println!("Device auth not confirmed, proceeding anyway");
    }

    Ok(AgentConnection { tx, rx })
}

pub fn load_or_create_device_keys() -> anyhow::Result<(ed25519_dalek::SigningKey, String)> {
    use base64::Engine;
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = home.join(".openclaw").join("claw-pen-orchestrator-device.json");

    if path.exists() {
        let data = std::fs::read_to_string(&path)?;
        let keys: serde_json::Value = serde_json::from_str(&data)?;
        let private_key_b64 = keys["privateKey"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing privateKey"))?;
        let private_key_bytes = base64::engine::general_purpose::STANDARD.decode(private_key_b64)?;
        let bytes: [u8; 32] = private_key_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&bytes);
        let device_id = keys["deviceId"].as_str().unwrap_or("unknown").to_string();
        return Ok((signing_key, device_id));
    }

    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, verifying_key.to_bytes());
    let device_id = hex::encode(sha2::Digest::finalize(hasher));

    let keys_json = serde_json::json!({
        "privateKey": base64::engine::general_purpose::STANDARD.encode(signing_key.to_bytes()),
        "publicKey": base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes()),
        "deviceId": device_id
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&keys_json)?)?;
    println!("Created new device identity: {}", &device_id[..16]);

    Ok((signing_key, device_id))
}

pub fn build_device_connect_request(req_id: &str, nonce: &str, signing_key: &ed25519_dalek::SigningKey, device_id: &str, gateway_token: Option<&str>, device_token: Option<&str>) -> serde_json::Value {
    use ed25519_dalek::Signer;
    use base64::Engine;

    let signed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let scopes_array = [
        "operator.admin",
        "operator.approvals",
        "operator.pairing",
        "operator.read",
        "operator.write",
    ];
    let scopes = scopes_array.join(",");
    let token_str = gateway_token
        .or(device_token)
        .unwrap_or("");

    let platform = "rust";
    let device_family = "desktop";
    let message = format!(
        "v3|{}|cli|cli|operator|{}|{}|{}|{}|{}|{}",
        device_id, scopes, signed_at, token_str, nonce, platform, device_family
    );

    println!("[device-auth] Signing message (v3): {}", &message);

    let signature = signing_key.sign(message.as_bytes());
    let b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let signature_b64 = b64url.encode(signature.to_bytes());
    let public_key_b64 = b64url.encode(signing_key.verifying_key().to_bytes());

    let mut params = serde_json::json!({
        "minProtocol": 4,
        "maxProtocol": 4,
        "client": {
            "id": "cli",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": "rust",
            "mode": "cli",
            "deviceFamily": "Desktop"
        },
        "role": "operator",
        "scopes": scopes_array,
        "device": {
            "id": device_id,
            "publicKey": public_key_b64,
            "signature": signature_b64,
            "signedAt": signed_at,
            "nonce": nonce
        },
        "caps": [],
        "commands": []
    });

    let mut auth = serde_json::Map::new();
    if let Some(token) = gateway_token {
        // Send both token (for signature verification) and password (for password mode)
        auth.insert("token".to_string(), serde_json::json!(token));
        auth.insert("password".to_string(), serde_json::json!(token));
    }
    // Compute device token for pre-paired device validation
    let token_input = format!("openclaw-device-token:{}", device_id);
    let token_hash = <sha2::Sha256 as sha2::Digest>::digest(token_input.as_bytes());
    let device_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_hash);
    auth.insert("deviceToken".to_string(), serde_json::json!(device_token));
    if !auth.is_empty() {
        params["auth"] = serde_json::Value::Object(auth);
    }

    serde_json::json!({
        "type": "req",
        "id": req_id,
        "method": "connect",
        "params": params
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Connecting to agent at 127.0.0.1:18800...");

    match connect_to_agent_with_token(18800, Some("clawpen")).await {
        Ok(_) => {
            println!("✅ Connected to agent!");
        }
        Err(e) => {
            println!("❌ Connection failed: {}", e);
        }
    }

    Ok(())
}
