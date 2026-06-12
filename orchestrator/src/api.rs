//! API handlers for Claw Pen Chat Server

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Response, Sse},
    Json,
};
use axum::response::sse::Event;
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

// Re-export chat types
pub use crate::chat_db::{ChatConversation as Conversation, ChatMessage};

// === Health ===

pub async fn health(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "claw-pen-chat"
    }))
}

// === Auth ===

#[derive(Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub role: String,
    pub username: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let auth = state.auth.read().await;
    let token_response = auth
        .login(&req.password)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    let username = auth
        .validate_token(&token_response.access_token)
        .map(|c| c.sub.clone())
        .unwrap_or_else(|_| "admin".to_string());
    drop(auth);

    Ok(Json(LoginResponse {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        role: "admin".to_string(),
        username,
    }))
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub secret_word: Option<String>,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut auth = state.auth.write().await;
    let _result = auth
        .register(&req.password)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    drop(auth);

    Ok(Json(serde_json::json!({
        "success": true,
        "username": req.username,
    })))
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let auth = state.auth.read().await;
    let access_token = auth
        .refresh(&req.refresh_token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    Ok(Json(serde_json::json!({
        "access_token": access_token,
    })))
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    Ok(Json(serde_json::json!({
        "username": claims.sub,
        "role": claims.role.unwrap_or_else(|| "user".to_string()),
    })))
}

fn extract_token(headers: &axum::http::HeaderMap) -> Result<String, (StatusCode, String)> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing authorization header".to_string()))
}

// === API Keys ===

#[derive(Deserialize)]
pub struct SetApiKeyRequest {
    pub provider: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct ApiKeyInfo {
    pub provider: String,
    pub has_key: bool,
}

pub async fn list_api_keys(State(state): State<Arc<AppState>>) -> Json<Vec<ApiKeyInfo>> {
    let keys = state.api_keys.read().await;
    let providers = ["zai", "anthropic", "openai", "kimi", "google"];

    Json(
        providers
            .iter()
            .map(|p| ApiKeyInfo {
                provider: p.to_string(),
                has_key: keys.contains_key(*p),
            })
            .collect(),
    )
}

pub async fn set_api_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetApiKeyRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut keys = state.api_keys.write().await;
    keys.insert(req.provider.clone(), req.key);

    let keys_path = state.data_dir.join("api_keys.json");
    if let Ok(json) = serde_json::to_string_pretty(&*keys) {
        let _ = std::fs::write(&keys_path, json);
    }

    Ok(StatusCode::CREATED)
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut keys = state.api_keys.write().await;
    keys.remove(&provider);

    let keys_path = state.data_dir.join("api_keys.json");
    if let Ok(json) = serde_json::to_string_pretty(&*keys) {
        let _ = std::fs::write(&keys_path, json);
    }

    Ok(StatusCode::NO_CONTENT)
}

// === Conversations ===

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn list_conversations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<Conversation>>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    // Ensure user exists in chat_db (legacy admin tokens don't have a user record)
    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let convs = state
        .chat_db
        .list_conversations(&claims.sub)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(convs))
}

pub async fn create_conversation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<Conversation>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    // Ensure the user exists in chat_db (legacy admin tokens don't have a user record)
    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conv = state
        .chat_db
        .create_conversation(&claims.sub, req.title.as_deref().unwrap_or("New Chat"))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(conv))
}

pub async fn get_conversation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Conversation>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    // Ensure user exists in chat_db (legacy admin tokens don't have a user record)
    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conv = state
        .chat_db
        .get_conversation(&id, &claims.sub)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(conv))
}

pub async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    // Ensure user exists in chat_db (legacy admin tokens don't have a user record)
    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state
        .chat_db
        .delete_conversation(&id, &claims.sub)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub color: Option<String>,
}

pub async fn update_conversation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(title) = req.title {
        state
            .chat_db
            .update_conversation_title(&id, &claims.sub, &title)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    if let Some(color) = req.color {
        state
            .chat_db
            .update_conversation_color(&id, &claims.sub, &color)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(serde_json::json!({"status": "updated"})))
}

/// Truncate message history to stay within a token budget.
/// Heuristic: ~4 chars per token for English. Drops oldest messages first.
fn truncate_to_budget(messages: Vec<ChatMessage>, max_tokens: usize) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return messages;
    }
    let max_chars = max_tokens * 4; // rough heuristic
    let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
    if total_chars <= max_chars {
        return messages;
    }
    // Drop oldest messages until under budget, but always keep at least the last message
    let mut chars = total_chars;
    let mut skip = 0;
    while chars > max_chars && skip < messages.len().saturating_sub(1) {
        chars -= messages[skip].content.len();
        skip += 1;
    }
    messages.into_iter().skip(skip).collect()
}

// === Compact ===

pub async fn compact_conversation(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ChatMessage>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    do_compact_conversation(&state, &id, &claims.sub).await
}

async fn do_compact_conversation(
    state: &Arc<AppState>,
    id: &str,
    user_id: &str,
) -> Result<Json<ChatMessage>, (StatusCode, String)> {
    // Get all messages
    let history = state
        .chat_db
        .get_messages(id, user_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if history.is_empty() {
        return Ok(Json(ChatMessage {
            id: "compact-empty".to_string(),
            role: "assistant".to_string(),
            content: "Nothing to compact — this conversation is empty.".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }));
    }

    // Build summarization prompt
    let conv_text = history
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let summary_prompt = format!(
        "Summarize the following conversation into a single concise paragraph. \
         Capture the key topics, questions, and conclusions. Keep it under 200 words. \
         This summary will replace the full conversation history.\n\n{}",
        conv_text
    );

    // Get API key
    let provider = "kimi";
    let keys = state.api_keys.read().await;
    let api_key = keys
        .get(provider)
        .cloned()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("No API key for provider: {}", provider)))?;
    drop(keys);

    // Call LLM for summary
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.kimi.com/coding/v1/messages")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "kimi-k2.6",
            "messages": [
                {"role": "user", "content": summary_prompt}
            ],
            "stream": false,
            "max_tokens": 512,
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM request failed: {}", e)))?;

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to parse LLM response: {}", e)))?;

    let summary = response_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
                .into()
        })
        .or_else(|| {
            response_json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "[Summary unavailable]".to_string());

    // Clear all messages
    state
        .chat_db
        .clear_messages(id, user_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Store summary as a system message
    let summary_msg = state
        .chat_db
        .add_message(id, user_id, "system", &summary)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update conversation title
    state
        .chat_db
        .update_conversation_title(id, user_id, "Compacted")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(summary_msg))
}

// === Messages ===

pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Query(pagination): Query<MessagePagination>,
) -> Result<Json<Vec<ChatMessage>>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let limit = pagination.limit.unwrap_or(100).min(500);
    let offset = pagination.offset.unwrap_or(0);

    let messages = state
        .chat_db
        .get_messages_paginated(&id, &claims.sub, limit, offset)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(messages))
}

#[derive(Deserialize)]
pub struct MessagePagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<ChatMessage>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    // Ensure user exists in chat_db (legacy admin tokens don't have a user record)
    let role = claims.role.as_deref().unwrap_or("admin");
    let user_role = crate::chat_db::UserRole::parse(role);
    state
        .chat_db
        .get_or_create_user_from_claims(&claims.sub, None, user_role)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Check for compact command
    if req.content.trim().eq_ignore_ascii_case("/compact") {
        return do_compact_conversation(&state, &id, &claims.sub).await;
    }

    // Store user message
    let _user_msg = state
        .chat_db
        .add_message(&id, &claims.sub, "user", &req.content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get conversation history for context
    let history = state
        .chat_db
        .get_messages(&id, &claims.sub)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Apply token budget: keep system prompt + recent history
    let history = truncate_to_budget(history, 8000); // ~8K tokens budget

    // Build messages for LLM
    let mut llm_messages: Vec<serde_json::Value> = vec![];

    // Add per-conversation system prompt first, then per-user, then global active
    let conv_prompt = state.chat_db.get_conversation_system_prompt(&id).ok().flatten();
    let user_prompt = state.chat_db.get_user_system_prompt(&claims.sub).ok().flatten();
    let global_prompt = state.chat_db.get_active_system_prompt().ok().flatten();
    if let Some(prompt) = conv_prompt.or(user_prompt).or(global_prompt) {
        llm_messages.push(serde_json::json!({
            "role": "system",
            "content": prompt,
        }));
    }

    llm_messages.extend(history.iter().map(|m| {
        serde_json::json!({
            "role": m.role,
            "content": m.content,
        })
    }));

    // Determine provider and model
    let provider = req.provider.unwrap_or_else(|| "kimi".to_string());
    let model = req
        .model
        .unwrap_or_else(|| "kimi-k2.6".to_string());

    // Get API key
    let keys = state.api_keys.read().await;
    let api_key = keys
        .get(&provider)
        .cloned()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("No API key for provider: {}", provider)))?;
    drop(keys);

    // Call Kimi API (or other provider)
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.kimi.com/coding/v1/messages")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": llm_messages,
            "stream": false,
            "max_tokens": 4096,
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM request failed: {}", e)))?;

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to parse LLM response: {}", e)))?;

    let assistant_content = response_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
                .into()
        })
        .or_else(|| {
            response_json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            response_json.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("[No response]")
                .to_string()
        });

    // Store assistant message
    let assistant_msg = state
        .chat_db
        .add_message(&id, &claims.sub, "assistant", &assistant_content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(assistant_msg))
}

// === Streaming Chat ===

#[derive(Deserialize)]
pub struct ChatStreamRequest {
    pub conversation_id: String,
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Json<ChatMessage>, (StatusCode, String)> {
    // For now, non-streaming. Store user message and return assistant response.
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let _user_msg = state
        .chat_db
        .add_message(&req.conversation_id, &claims.sub, "user", &req.content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let history = state
        .chat_db
        .get_messages(&req.conversation_id, &claims.sub)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let llm_messages: Vec<serde_json::Value> = history
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let provider = req.provider.unwrap_or_else(|| "kimi".to_string());
    let model = req.model.unwrap_or_else(|| "kimi-k2.6".to_string());

    let keys = state.api_keys.read().await;
    let api_key = keys
        .get(&provider)
        .cloned()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("No API key for provider: {}", provider)))?;
    drop(keys);

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.kimi.com/coding/v1/messages")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": llm_messages,
            "stream": false,
            "max_tokens": 4096,
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM request failed: {}", e)))?;

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to parse LLM response: {}", e)))?;

    let content = response_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
                .into()
        })
        .or_else(|| {
            response_json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            response_json.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("[No response]")
                .to_string()
        });

    let assistant_msg = state
        .chat_db
        .add_message(&req.conversation_id, &claims.sub, "assistant", &content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(assistant_msg))
}

// === WebSocket Chat ===

pub async fn chat_websocket(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    let token = params
        .get("token")
        .ok_or((StatusCode::UNAUTHORIZED, "Missing token".to_string()))?;

    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let user_id = claims.sub.clone();
    let state = state.clone();

    Ok(ws.on_upgrade(move |socket| handle_chat_ws(socket, state, user_id)))
}

async fn handle_chat_ws(
    mut socket: axum::extract::ws::WebSocket,
    state: Arc<AppState>,
    user_id: String,
) {
    use axum::extract::ws::Message;

    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(req) = serde_json::from_str::<ChatStreamRequest>(&text) {
                    // Store user message
                    let _ = state.chat_db.add_message(
                        &req.conversation_id,
                        &user_id,
                        "user",
                        &req.content,
                    );

                    // Get history
                    let history = match state.chat_db.get_messages(&req.conversation_id, &user_id) {
                        Ok(h) => h,
                        Err(_) => continue,
                    };

                    let llm_messages: Vec<serde_json::Value> = history
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "role": m.role,
                                "content": m.content,
                            })
                        })
                        .collect();

                    let provider = req.provider.unwrap_or_else(|| "kimi".to_string());
                    let model = req.model.unwrap_or_else(|| "kimi-k2.6".to_string());

                    let keys = state.api_keys.read().await;
                    let api_key = match keys.get(&provider) {
                        Some(k) => k.clone(),
                        None => continue,
                    };
                    drop(keys);

                    // Non-streaming for WebSocket simplicity
                    let client = reqwest::Client::new();
                    if let Ok(response) = client
                        .post("https://api.kimi.com/coding/v1/messages")
                        .header("Authorization", format!("Bearer {}", api_key))
                        .header("Content-Type", "application/json")
                        .json(&serde_json::json!({
                            "model": model,
                            "messages": llm_messages,
                            "stream": false,
                            "max_tokens": 4096,
                        }))
                        .send()
                        .await
                    {
                        if let Ok(response_json) = response.json::<serde_json::Value>().await {
                            let content = response_json
                                .get("choices")
                                .and_then(|c| c.as_array())
                                .and_then(|c| c.first())
                                .and_then(|c| c.get("message"))
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str())
                                .unwrap_or("[No response]");

                            // Store assistant message
                            let _ = state.chat_db.add_message(
                                &req.conversation_id,
                                &user_id,
                                "assistant",
                                content,
                            );

                            let _ = socket
                                .send(Message::Text(
                                    serde_json::json!({
                                        "role": "assistant",
                                        "content": content,
                                        "conversation_id": req.conversation_id,
                                    })
                                    .to_string(),
                                ))
                                .await;
                        }
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}

// === Teams ===

pub async fn list_teams(State(state): State<Arc<AppState>>) -> Json<Vec<serde_json::Value>> {
    let teams = state.teams.list_teams();
    Json(teams)
}

pub async fn list_team_roles(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    let roles = state.teams.list_roles(&id);
    Json(roles)
}

// === Admin ===

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    if claims.role.as_deref() != Some("admin") && claims.role.as_deref() != Some("teacher") {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    }
    drop(auth);

    let users = state.chat_db.list_all_users()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let json_users: Vec<serde_json::Value> = users.into_iter().map(|u| {
        serde_json::json!({
            "id": u.id,
            "username": u.username,
            "display_name": u.display_name,
            "role": u.role.as_str(),
            "approval_status": u.approval_status.as_str(),
            "created_at": u.created_at,
        })
    }).collect();

    Ok(Json(json_users))
}

#[derive(Deserialize)]
pub struct ApproveUserRequest {
    pub username: String,
}

pub async fn approve_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ApproveUserRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    if claims.role.as_deref() != Some("admin") && claims.role.as_deref() != Some("teacher") {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    }
    drop(auth);

    // Find user by username
    let user = state.chat_db.get_user_by_username(&req.username)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "User not found".to_string()))?;

    state.chat_db.update_user_status(&user.id, crate::chat_db::ApprovalStatus::Approved)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth
        .validate_token(&token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    if claims.role.as_deref() != Some("admin") {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    }
    drop(auth);

    state.chat_db.delete_user(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_conversation_system_prompt(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = extract_token(&headers)?;
    let auth = state.auth.read().await;
    let claims = auth.validate_token(&token).map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    drop(auth);

    let prompt = req.get("prompt").and_then(|v| v.as_str());
    state.chat_db.set_conversation_system_prompt(&id, prompt)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"status": "ok"})))
}

pub async fn list_system_prompt_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let prompts = state.chat_db.list_system_prompts()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let json_prompts = prompts.into_iter().map(|p| {
        serde_json::json!({
            "id": p.id,
            "name": p.name,
            "content": p.content,
            "is_active": p.is_active,
        })
    }).collect();

    Ok(Json(json_prompts))
}
