//! Direct LLM backend — proxies chat to OpenAI-compatible endpoints with
//! native streaming. No container, no openclaw, no plugin pipeline.
//!
//! Contract:
//!   - Agent's `runtime` is set to `"direct"`.
//!   - `config.api_key` (or globally stored key for the provider) authenticates.
//!   - `config.llm_model` is the model id sent to the provider.
//!   - `config.env_vars["LLM_BASE_URL"]` overrides the default endpoint
//!     (used for Ollama / LM Studio / self-hosted vLLM / sovereign registries).
//!   - System prompt comes from `data/agents/<name>/identity/system_prompt.md`
//!     if present, else the template default.
//!   - Streams tokens to the browser as `chat`/`delta` events (already handled
//!     by the Tauri client).

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use crate::types::{AgentContainer, ConversationMessage, LlmProvider};

/// Wire format spoken by an upstream provider.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ApiFormat {
    /// OpenAI chat completions: POST /chat/completions, "data: {choices:[{delta:{content}}]}".
    OpenAi,
    /// Anthropic messages: POST /messages, SSE events with content_block_delta.
    AnthropicMessages,
}

/// Default base URL + wire format per provider.
fn default_endpoint(provider: &LlmProvider) -> (&'static str, ApiFormat) {
    match provider {
        LlmProvider::OpenAI       => ("https://api.openai.com/v1",                       ApiFormat::OpenAi),
        // Kimi Code uses Anthropic-format on api.kimi.com/coding (v1/messages).
        // Moonshot Open Platform uses OpenAI-format on api.moonshot.cn/v1.
        LlmProvider::Kimi       => ("https://api.moonshot.cn/v1",                    ApiFormat::OpenAi),
        LlmProvider::KimiCode   => ("https://api.kimi.com/coding",                   ApiFormat::AnthropicMessages),
        LlmProvider::Anthropic    => ("https://api.anthropic.com",                       ApiFormat::AnthropicMessages),
        LlmProvider::Zai          => ("https://api.z.ai/api/coding/paas/v4",             ApiFormat::OpenAi),
        LlmProvider::Gemini       => ("https://generativelanguage.googleapis.com/v1beta/openai", ApiFormat::OpenAi),
        LlmProvider::Huggingface  => ("https://api-inference.huggingface.co/v1",         ApiFormat::OpenAi),
        LlmProvider::Ollama       => ("http://host.docker.internal:11434/v1",            ApiFormat::OpenAi),
        LlmProvider::Lmstudio     => ("http://host.docker.internal:1234/v1",             ApiFormat::OpenAi),
        LlmProvider::Vllm         => ("http://localhost:8000/v1",                        ApiFormat::OpenAi),
        LlmProvider::LlamaCpp     => ("http://localhost:8080/v1",                        ApiFormat::OpenAi),
        _                         => ("",                                                ApiFormat::OpenAi),
    }
}

/// Some providers expect a different model id on the wire than the one users
/// configure. Translate here.
fn wire_model_id(provider: &LlmProvider, configured: &str) -> String {
    // Pass through as-is. Provider-specific translations can be added here
    // if the upstream catalog uses different ids than the UI templates.
    let _ = provider; // silence unused warning if no translations exist
    configured.to_string()
}

// ─── OpenAI chat/completions wire types ─────────────────────────────────────
#[derive(Debug, Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiChunk {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    delta: OpenAiDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
}

// ─── Anthropic /v1/messages wire types ──────────────────────────────────────
#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// SSE event body when type=content_block_delta.
#[derive(Debug, Deserialize)]
struct AnthropicSseEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(default)]
    delta: Option<AnthropicEventDelta>,
}

#[derive(Debug, Deserialize)]
struct AnthropicEventDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    stop_reason: Option<String>,
}

/// Read the teacher-authored system prompt from the agent's identity volume.
async fn load_system_prompt(agent_name: &str) -> Option<String> {
    let path = std::path::PathBuf::from("./data/agents")
        .join(agent_name)
        .join("identity")
        .join("system_prompt.md");
    tokio::fs::read_to_string(path).await.ok()
}

/// Resolve the API key for an agent: per-agent config → global stored key → env.
async fn resolve_api_key(state: &Arc<AppState>, agent: &AgentContainer) -> Option<String> {
    if let Some(k) = &agent.config.api_key {
        if !k.is_empty() {
            return Some(k.clone());
        }
    }
    let key_lookup = match agent.config.llm_provider {
        LlmProvider::OpenAI => "openai",
        LlmProvider::Anthropic => "anthropic",
        LlmProvider::Kimi => "kimi",
        LlmProvider::KimiCode => "kimi-code",
        LlmProvider::Zai => "zai",
        LlmProvider::Gemini => "google",
        LlmProvider::Huggingface => "huggingface",
        _ => return None,
    };
    let keys = state.api_keys.read().await;
    // Return the provider-specific key if present, otherwise fall back
    // across the two Kimi key names so either works for both providers.
    keys.get(key_lookup).cloned().or_else(|| {
        if agent.config.llm_provider == LlmProvider::Kimi {
            keys.get("kimi-code").cloned()
        } else if agent.config.llm_provider == LlmProvider::KimiCode {
            keys.get("kimi").cloned()
        } else {
            None
        }
    })
}

/// Send `chat`/`final` event with full text and persist the assistant turn.
async fn finalize(
    client_tx: &mut futures::stream::SplitSink<WebSocket, Message>,
    full_text: &str,
    agent_id: &str,
    agent_name: &str,
    session_id: &str,
) {
    use futures::SinkExt;
    let final_event = json!({
        "type": "event",
        "event": "chat",
        "payload": {
            "state": "final",
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": full_text }]
            }
        }
    });
    let _ = client_tx.send(Message::Text(final_event.to_string())).await;

    let msg = ConversationMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "assistant".to_string(),
        content: full_text.to_string(),
        agent_id: agent_id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        metadata: Default::default(),
    };
    if let Err(e) = crate::api::append_conversation_message(agent_name, &msg).await {
        tracing::warn!("Direct: failed to persist assistant message: {}", e);
    }
}

async fn send_error(
    client_tx: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &str,
) {
    use futures::SinkExt;
    let err = json!({
        "role": "system",
        "error": "direct backend error",
        "content": msg,
        "timestamp": chrono::Utc::now().timestamp()
    });
    let _ = client_tx.send(Message::Text(err.to_string())).await;
}

/// Handle a chat WebSocket for a direct-runtime agent.
///
/// Connection model: each user message starts a fresh streaming HTTP
/// completion to the upstream provider. Conversation history is loaded from
/// the JSONL persistence layer so the LLM has context across turns.
pub async fn handle_direct_chat(
    socket: WebSocket,
    state: Arc<AppState>,
    agent_id: String,
    agent_name: String,
    session_id: String,
) {
    use futures::SinkExt;
    use futures::stream::StreamExt as _;

    let (mut client_tx, mut client_rx) = socket.split();

    // Look up the agent and its config.
    let agent = {
        let containers = state.containers.read().await;
        match containers.iter().find(|c| c.id == agent_id || c.name == agent_name) {
            Some(a) => a.clone(),
            None => {
                send_error(&mut client_tx, "agent not found").await;
                return;
            }
        }
    };

    let api_key = match resolve_api_key(&state, &agent).await {
        Some(k) => k,
        None => {
            send_error(&mut client_tx, "no API key configured for this provider").await;
            return;
        }
    };

    let (default_url, default_format) = default_endpoint(&agent.config.llm_provider);
    let base_url = agent.config.env_vars.get("LLM_BASE_URL")
        .cloned()
        .unwrap_or_else(|| default_url.to_string());
    if base_url.is_empty() {
        send_error(&mut client_tx, "no LLM_BASE_URL configured for this provider").await;
        return;
    }
    let api_format = match agent.config.env_vars.get("LLM_API_FORMAT").map(|s| s.to_lowercase()) {
        Some(s) if s == "openai" => ApiFormat::OpenAi,
        Some(s) if s == "anthropic" || s == "anthropic-messages" => ApiFormat::AnthropicMessages,
        _ => default_format,
    };

    let configured_model = agent.config.llm_model
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let model = wire_model_id(&agent.config.llm_provider, &configured_model);

    let system_prompt = load_system_prompt(&agent_name).await;

    // Send the initial connection ack the Tauri client expects.
    let ack = json!({
        "role": "system",
        "content": "Connected to agent (direct backend)",
        "type": "event",
        "event": "connection.established",
        "timestamp": chrono::Utc::now().timestamp()
    });
    if client_tx.send(Message::Text(ack.to_string())).await.is_err() {
        return;
    }

    let endpoint = match api_format {
        ApiFormat::OpenAi => format!("{}/chat/completions", base_url.trim_end_matches('/')),
        ApiFormat::AnthropicMessages => format!("{}/v1/messages", base_url.trim_end_matches('/')),
    };
    let http = reqwest::Client::new();

    while let Some(msg_result) = client_rx.next().await {
        let text = match msg_result {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => break,
            _ => continue,
        };

        // Parse client message — accept either {content: ...} or
        // {type: "req", method: "chat.send", params: {message: ...}}.
        let parsed = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let user_content = parsed.get("content").and_then(|v| v.as_str())
            .or_else(|| parsed.pointer("/params/message").and_then(|v| v.as_str()))
            .or_else(|| parsed.pointer("/payload/content").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        if user_content.is_empty() {
            continue;
        }

        // Persist user turn.
        let user_msg = ConversationMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            role: "user".to_string(),
            content: user_content.clone(),
            agent_id: agent_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: Default::default(),
        };
        if let Err(e) = crate::api::append_conversation_message(&agent_name, &user_msg).await {
            tracing::warn!("Direct: failed to persist user message: {}", e);
        }

        // Build messages: system + prior session turns + this user turn.
        let history = crate::api::load_conversation_messages(&agent_name, &session_id)
            .unwrap_or_default();

        // Send streaming start event.
        let start_event = json!({
            "type": "event",
            "event": "agent",
            "payload": {
                "stream": "lifecycle",
                "data": { "phase": "start" }
            }
        });
        let _ = client_tx.send(Message::Text(start_event.to_string())).await;

        let total_chars: usize = history.iter().map(|h| h.content.len()).sum::<usize>()
            + system_prompt.as_deref().map(|s| s.len()).unwrap_or(0)
            + user_content.len();
        tracing::info!(
            "[direct] POST {} model={} format={:?} chars={}",
            endpoint, model, api_format, total_chars
        );

        // Build and send request based on format.
        let resp = match api_format {
            ApiFormat::OpenAi => {
                let mut messages = Vec::new();
                if let Some(sp) = system_prompt.as_deref() {
                    messages.push(OpenAiMessage { role: "system", content: sp });
                }
                for h in &history {
                    messages.push(OpenAiMessage {
                        role: h.role.as_str(),
                        content: h.content.as_str(),
                    });
                }
                if history.is_empty() {
                    messages.push(OpenAiMessage { role: "user", content: &user_content });
                }
                let body = OpenAiRequest { model: &model, messages, stream: true };
                http.post(&endpoint)
                    .bearer_auth(&api_key)
                    .json(&body)
                    .send()
                    .await
            }
            ApiFormat::AnthropicMessages => {
                // Anthropic format: system goes outside `messages`, only user/assistant
                // turns inside. No `system` role inside messages array.
                let mut messages = Vec::new();
                for h in &history {
                    if h.role == "system" {
                        continue;
                    }
                    messages.push(AnthropicMessage {
                        role: h.role.as_str(),
                        content: h.content.as_str(),
                    });
                }
                if messages.is_empty() {
                    messages.push(AnthropicMessage { role: "user", content: &user_content });
                }
                let body = AnthropicRequest {
                    model: &model,
                    max_tokens: 4096,
                    messages,
                    system: system_prompt.as_deref(),
                    stream: true,
                };
                http.post(&endpoint)
                    .bearer_auth(&api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body)
                    .send()
                    .await
            }
        };

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                send_error(&mut client_tx, &format!("HTTP error: {}", e)).await;
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("[direct] provider error {}: {}", status, &body[..body.len().min(500)]);
            send_error(
                &mut client_tx,
                &format!("provider returned {}: {}", status, body),
            ).await;
            continue;
        }
        tracing::info!("[direct] provider OK, streaming response...");

        // SSE stream: parse lines, dispatch payloads by api_format.
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut full_text = String::new();

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(b) => b,
                Err(e) => {
                    send_error(&mut client_tx, &format!("stream error: {}", e)).await;
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl_idx) = buf.find('\n') {
                let line = buf[..nl_idx].trim_end_matches('\r').to_string();
                buf.drain(..=nl_idx);
                let payload = match line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")) {
                    Some(p) => p.trim(),
                    None => continue,
                };
                if payload.is_empty() || payload == "[DONE]" {
                    if payload == "[DONE]" { break 'outer; }
                    continue;
                }

                let delta_text: Option<String>;
                let stop: bool;

                match api_format {
                    ApiFormat::OpenAi => {
                        let parsed: OpenAiChunk = match serde_json::from_str(payload) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        let mut text = String::new();
                        let mut finished = false;
                        for choice in parsed.choices {
                            if let Some(d) = choice.delta.content {
                                text.push_str(&d);
                            }
                            if choice.finish_reason.is_some() {
                                finished = true;
                            }
                        }
                        delta_text = if text.is_empty() { None } else { Some(text) };
                        stop = finished;
                    }
                    ApiFormat::AnthropicMessages => {
                        let parsed: AnthropicSseEvent = match serde_json::from_str(payload) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        match parsed.event_type.as_deref() {
                            Some("content_block_delta") => {
                                let t = parsed.delta.as_ref()
                                    .and_then(|d| d.text.clone());
                                delta_text = t;
                                stop = false;
                            }
                            Some("message_stop") => {
                                delta_text = None;
                                stop = true;
                            }
                            Some("message_delta") => {
                                let s = parsed.delta.as_ref()
                                    .and_then(|d| d.stop_reason.clone());
                                delta_text = None;
                                stop = s.is_some();
                            }
                            _ => continue,
                        }
                    }
                }

                if let Some(d) = delta_text {
                    if !d.is_empty() {
                        full_text.push_str(&d);
                        let delta_event = json!({
                            "type": "event",
                            "event": "chat",
                            "payload": {
                                "state": "delta",
                                "message": {
                                    "role": "assistant",
                                    "content": d
                                }
                            }
                        });
                        if client_tx.send(Message::Text(delta_event.to_string())).await.is_err() {
                            break 'outer;
                        }
                    }
                }
                if stop { break 'outer; }
            }
        }

        // Send lifecycle end + final message + persist.
        let end_event = json!({
            "type": "event",
            "event": "agent",
            "payload": {
                "stream": "lifecycle",
                "data": { "phase": "end" }
            }
        });
        let _ = client_tx.send(Message::Text(end_event.to_string())).await;

        finalize(&mut client_tx, &full_text, &agent_id, &agent_name, &session_id).await;
    }
}
