use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    body::Body,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, error};
use futures_util::stream::{self, StreamExt};

use crate::inference::InferenceEngine;
use crate::model::{GenerateRequest, SamplingParams};

/// OpenAI-compatible chat request
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible chat response
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// OpenAI-compatible models response
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub owned_by: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Serialize)]
pub struct Permission {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub allow_create_engine: bool,
    pub allow_sampling: bool,
    pub allow_logprobs: bool,
}

/// SSE event for streaming
#[derive(Debug, Serialize)]
struct StreamEvent {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Serialize)]
struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct StreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Create a new API server
pub struct InferenceApi {
    engine: Arc<InferenceEngine>,
    port: u16,
}

impl InferenceApi {
    pub fn new(engine: Arc<InferenceEngine>, port: u16) -> Self {
        Self { engine, port }
    }

    /// Build Axum router
    pub fn router(self) -> axum::Router {
        axum::Router::new()
            .route("/v1/chat/completions", post(chat_completions))
            .route("/v1/models", axum::routing::get(list_models))
            .with_state(Arc::new(self))
    }

    /// Run the API server
    pub async fn run(self) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let app = self.router();

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        info!("Inference API listening on {}", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Get the engine
    pub fn engine(&self) -> &Arc<InferenceEngine> {
        &self.engine
    }
}

/// Chat completions endpoint
async fn chat_completions(
    State(api): State<Arc<InferenceApi>>,
    Json(req): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    info!("Chat completion request for model: {}", req.model);

    // Convert messages to a single prompt
    let prompt = req.messages
        .iter()
        .map(|m| format!("{}: {}\n", m.role, m.content))
        .collect::<Vec<_>>()
        .join("");

    // Build sampling params
    let sampling = SamplingParams {
        temperature: req.temperature.unwrap_or(0.7) as f32,
        top_p: req.top_p.map(|v| v as f32),
        top_k: Some(40),
        repeat_penalty: 1.0,
        repeat_last_n: 64,
    };

    let request = GenerateRequest {
        prompt,
        sampling,
        max_tokens: req.max_tokens.map(|x| x as usize),
    };

    // Handle streaming vs non-streaming
    if req.stream.unwrap_or(false) {
        // Return SSE stream
        let engine = api.engine().clone();
        let model_name = req.model.clone();

        // For streaming, we'll generate the full response and then stream it
        // This is a simplified approach - true streaming would require token-level generation
        let response_chunks = if let Ok(response) = engine.generate(request).await {
            let created = now_secs();
            let req_id = new_uuid();
            let mut chunks = Vec::new();

            // Stream character by character
            for chunk in response.text.chars() {
                let chunk_str = chunk.to_string();
                let event = StreamEvent {
                    id: req_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model_name.clone(),
                    choices: vec![StreamChoice {
                        index: 0,
                        delta: StreamDelta {
                            content: Some(chunk_str),
                            role: None,
                        },
                        finish_reason: None,
                    }],
                };

                if let Ok(json) = serde_json::to_string(&event) {
                    chunks.push(format!("data: {}\n\n", json));
                }
            }

            // Send final chunk with finish_reason
            let final_event = StreamEvent {
                id: req_id,
                object: "chat.completion.chunk".to_string(),
                created,
                model: model_name,
                choices: vec![StreamChoice {
                    index: 0,
                    delta: StreamDelta {
                        content: None,
                        role: None,
                    },
                    finish_reason: Some("stop".to_string()),
                }],
            };

            if let Ok(json) = serde_json::to_string(&final_event) {
                chunks.push(format!("data: {}\n\n", json));
            }

            chunks
        } else {
            Vec::new()
        };

        let stream = stream::iter(response_chunks);

        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .body(Body::from_stream(stream.map(|s| Ok::<_, Box<dyn std::error::Error + Send + Sync>>(s))))
            .unwrap()
    } else {
        // Non-streaming response
        match api.engine().generate(request).await {
            Ok(response) => {
                let chat_response = ChatCompletionResponse {
                    id: new_uuid(),
                    object: "chat.completion".to_string(),
                    created: now_secs(),
                    model: req.model,
                    choices: vec![Choice {
                        index: 0,
                        message: ChatMessage {
                            role: "assistant".to_string(),
                            content: response.text,
                        },
                        finish_reason: response.finish_reason,
                    }],
                    usage: Usage {
                        prompt_tokens: 0, // TODO: Implement tokenization
                        completion_tokens: response.tokens_used as u32,
                        total_tokens: response.tokens_used as u32,
                    },
                };

                Json(chat_response).into_response()
            }
            Err(e) => {
                let error_msg = e.to_string();
                error!("Generation error: {}", error_msg);
                let error_json = serde_json::json!({
                    "error": {
                        "message": error_msg,
                        "type": "inference_error"
                    }
                });
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(error_json.to_string()))
                    .unwrap()
            }
        }
    }
}

/// List available models endpoint
async fn list_models(State(api): State<Arc<InferenceApi>>) -> impl IntoResponse {
    let _ = api; // Suppress unused warning

    let models = ModelsResponse {
        object: "list".to_string(),
        data: vec![ModelInfo {
            id: "qwen3.5-4b".to_string(),
            object: "model".to_string(),
            owned_by: "claw-pen-inference".to_string(),
            permissions: vec![Permission {
                id: format!("modelperm-{}", new_uuid()),
                object: "model".to_string(),
                created: now_secs(),
                allow_create_engine: true,
                allow_sampling: true,
                allow_logprobs: true,
            }],
        }],
    };

    Json(models).into_response()
}

/// Get current time as seconds since UNIX epoch
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Generate a simple UUID v4
fn new_uuid() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() / 1000;

    let time_low = (timestamp & 0xffffffff) as u32;
    let time_mid = ((timestamp >> 32) & 0xffff) as u16;
    let time_hi = ((timestamp >> 48) & 0xffff) as u16;
    let clock_seq = COUNTER.fetch_add(1, Ordering::SeqCst) as u16;

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        time_low,
        time_mid,
        (time_hi & 0x0fff),
        (clock_seq & 0xffff) | 0x8000,
        rand::random::<u64>() & 0xffffffffffff
    )
}
