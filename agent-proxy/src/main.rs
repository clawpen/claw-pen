use anyhow::{anyhow, Result};
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContentItem {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatEvent {
    #[serde(rename = "type")]
    event_type: String,
    event: String,
    payload: ChatPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatPayload {
    state: String,
    message: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatResponse {
    #[serde(rename = "type")]
    res_type: String,
    ok: bool,
    id: String,
    payload: ChatResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatResponsePayload {
    session: ChatSession,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatSession {
    id: String,
}

#[derive(Debug, Clone)]
struct AgentConfig {
    api_key: String,
    base_url: String,
    model: String,
    password: Option<String>,
}

#[derive(Debug, Clone)]
struct SessionState {
    messages: Vec<serde_json::Value>,
}

struct AgentState {
    config: AgentConfig,
    sessions: RwLock<HashMap<String, SessionState>>,
}

fn main() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        if let Err(e) = run().await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    });
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = load_config();
    info!("Agent proxy starting — model: {}, base_url: {}", config.model, config.base_url);

    let state = Arc::new(AgentState {
        config: config.clone(),
        sessions: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/", get(ws_handler))
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "18790".to_string());
    let addr = format!("0.0.0.0:{}", port);
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn load_config() -> AgentConfig {
    let api_key = env::var("KIMI_API_KEY")
        .or_else(|_| env::var("LLM_API_KEY"))
        .unwrap_or_default();
    let base_url = env::var("LLM_BASE_URL")
        .unwrap_or_else(|_| "https://api.kimi.com/coding/v1".to_string());
    let model = env::var("LLM_MODEL")
        .unwrap_or_else(|_| "kimi-k2.6".to_string());
    let password = env::var("GATEWAY_PASSWORD").ok();

    AgentConfig {
        api_key,
        base_url,
        model,
        password,
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AgentState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AgentState>) {
    let (mut sender, mut receiver) = socket.split();

    // Bare-bones mode: if no password is set, skip handshake entirely
    let password_required = state.config.password.is_some();

    if password_required {
        // Simple password auth — one round, no challenge nonce
        let auth_response = serde_json::json!({
            "type": "event",
            "event": "auth.required",
            "payload": {}
        });
        if let Err(e) = sender
            .send(axum::extract::ws::Message::Text(auth_response.to_string().into()))
            .await
        {
            error!("Failed to send auth required: {}", e);
            return;
        }

        // Wait for password
        let mut authenticated = false;
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                axum::extract::ws::Message::Text(text) => {
                    if let Ok(req) = serde_json::from_str::<serde_json::Value>(&text) {
                        if req.get("method").and_then(|m| m.as_str()) == Some("auth") {
                            let provided = req
                                .get("params")
                                .and_then(|p| p.get("password"))
                                .and_then(|p| p.as_str());
                            let expected = state.config.password.as_ref().unwrap();
                            authenticated = provided == Some(expected.as_str());

                            let response = serde_json::json!({
                                "type": "res",
                                "ok": authenticated,
                                "id": req.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                            });
                            if let Err(e) = sender
                                .send(axum::extract::ws::Message::Text(response.to_string().into()))
                                .await
                            {
                                error!("Failed to send auth response: {}", e);
                                return;
                            }
                            if authenticated {
                                info!("Agent authenticated");
                                break;
                            } else {
                                warn!("Authentication failed");
                                return;
                            }
                        }
                    }
                }
                axum::extract::ws::Message::Close(_) => return,
                _ => {}
            }
        }

        if !authenticated {
            warn!("Agent not authenticated, closing connection");
            return;
        }
    } else {
        // No password — send ready immediately
        let ready = serde_json::json!({
            "type": "event",
            "event": "ready",
            "payload": {}
        });
        if let Err(e) = sender
            .send(axum::extract::ws::Message::Text(ready.to_string().into()))
            .await
        {
            error!("Failed to send ready: {}", e);
            return;
        }
        info!("Agent connected (no auth required)");
    }

    // Handle chat messages
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            axum::extract::ws::Message::Text(text) => {
                if let Err(e) = handle_chat_message(&text, &mut sender, &state).await {
                    error!("Error handling chat message: {}", e);
                }
            }
            axum::extract::ws::Message::Close(_) => {
                info!("Client disconnected");
                break;
            }
            _ => {}
        }
    }
}

async fn handle_chat_message(
    text: &str,
    sender: &mut futures_util::stream::SplitSink<
        axum::extract::ws::WebSocket,
        axum::extract::ws::Message,
    >,
    state: &AgentState,
) -> Result<()> {
    let req: serde_json::Value = serde_json::from_str(text)?;

    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();

    if method != "chat.send" {
        // Send empty response for unknown methods
        let response = serde_json::json!({
            "type": "res",
            "ok": true,
            "id": id,
        });
        sender
            .send(axum::extract::ws::Message::Text(response.to_string().into()))
            .await?;
        return Ok(());
    }

    let params = req.get("params").ok_or_else(|| anyhow!("No params"))?;
    let session_key = params
        .get("sessionKey")
        .and_then(|s| s.as_str())
        .unwrap_or("default")
        .to_string();

    let message = params
        .get("message")
        .ok_or_else(|| anyhow!("No message"))?;
    let content = message
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| anyhow!("No content array"))?;

    let mut user_text = String::new();
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                user_text.push_str(text);
            }
        }
    }

    // Send ack response
    let response = ChatResponse {
        res_type: "res".to_string(),
        ok: true,
        id: id.clone(),
        payload: ChatResponsePayload {
            session: ChatSession {
                id: session_key.clone(),
            },
        },
    };
    sender
        .send(axum::extract::ws::Message::Text(
            serde_json::to_string(&response).unwrap().into(),
        ))
        .await?;

    // Call LLM API
    let mut sessions = state.sessions.write().await;
    let session = sessions.entry(session_key.clone()).or_insert_with(|| SessionState {
        messages: Vec::new(),
    });

    session.messages.push(serde_json::json!({
        "role": "user",
        "content": user_text,
    }));

    let messages = session.messages.clone();
    drop(sessions);

    let client = reqwest::Client::new();
    let api_req = serde_json::json!({
        "model": state.config.model,
        "messages": messages,
        "stream": true,
    });

    let mut api_response = client
        .post(format!("{}/chat/completions", state.config.base_url))
        .header("Authorization", format!("Bearer {}", state.config.api_key))
        .header("Content-Type", "application/json")
        .json(&api_req)
        .send()
        .await?;

    let mut full_response = String::new();

    while let Some(chunk) = api_response.chunk().await? {
        let text = String::from_utf8_lossy(&chunk);
        for line in text.lines() {
            if line.starts_with("data: ") {
                let data = line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")).unwrap_or("");
                if data == "[DONE]" {
                    break;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(choice) = event
                        .get("choices")
                        .and_then(|c| c.as_array())
                        .and_then(|c| c.first())
                    {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                full_response.push_str(content);
                            }
                            if let Some(_reasoning) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                                // Optionally prepend reasoning to response
                                // full_response.push_str(reasoning);
                            }
                        }
                        if choice.get("finish_reason").and_then(|f| f.as_str()) == Some("stop") {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Store assistant response
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(&session_key) {
        session.messages.push(serde_json::json!({
            "role": "assistant",
            "content": full_response.clone(),
        }));
    }

    // Send final event
    let event = ChatEvent {
        event_type: "event".to_string(),
        event: "chat".to_string(),
        payload: ChatPayload {
            state: "final".to_string(),
            message: ChatMessage {
                role: "assistant".to_string(),
                content: vec![ContentItem {
                    content_type: "text".to_string(),
                    text: full_response,
                }],
            },
        },
    };

    sender
        .send(axum::extract::ws::Message::Text(
            serde_json::to_string(&event).unwrap().into(),
        ))
        .await?;

    Ok(())
}
