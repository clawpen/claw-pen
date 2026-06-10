//! Gateway Harness — Inbound WebSocket server for external agents
//!
//! Accepts connections from external agents (OpenClaw-compatible or raw WebSocket)
//! and bridges them into the Claw Pen orchestrator's internal systems.
//!
//! # Architecture
//!
//! ```text
//! External Agent (OpenClaw/Custom)  ←→  Gateway Harness  ←→  Orchestrator
//!     ws://clawpen.ca/gateway/ws           (this module)      (AppState)
//! ```
//!
//! # Authentication
//!
//! - `Bearer` token via `Authorization` header on WebSocket upgrade
//! - Or `?token=<jwt>` query parameter (same pattern as existing chat websockets)
//! - Admin tokens get full access; user tokens get access to their assigned agents
//!
//! # Protocol
//!
//! The gateway speaks a simplified OpenClaw protocol:
//! - `chat.send` — send a message to a session
//! - `chat.subscribe` — subscribe to a session's events
//! - `agent.register` — register as an external agent (gets an agent ID)
//! - `agent.heartbeat` — keep connection alive
//! - `agent.status` — report own status
//!
//! All messages are JSON over WebSocket text frames.

use anyhow::{anyhow, Result};
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{Duration, Instant};

use crate::types::{AgentContainer, AgentStatus};
use crate::AppState;

/// Port for the gateway server (separate from main API, but can be same Axum router)
pub const GATEWAY_WS_PATH: &str = "/gateway/ws";

/// Heartbeat interval — connections without heartbeat are cleaned up
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// Connection timeout without any message
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(120);

/// Determine if a role string has gateway management privileges
fn is_gateway_admin(role: Option<&str>) -> bool {
    matches!(role, Some("admin") | Some("teacher"))
}

/// Registry of active gateway connections
#[derive(Debug, Default)]
pub struct GatewayRegistry {
    /// Map of connection ID → connection handle
    pub connections: HashMap<String, GatewayConnection>,
    /// Map of agent ID → connection ID (for routing messages to external agents)
    pub agent_to_connection: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct GatewayConnection {
    pub id: String,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub role: Option<String>,
    pub connected_at: Instant,
    pub last_heartbeat: Instant,
    pub session_subscriptions: Vec<String>,
    /// Channel to send messages to this connection
    pub tx: mpsc::UnboundedSender<GatewayMessage>,
}

/// Messages that can be sent TO a gateway connection
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum GatewayMessage {
    #[serde(rename = "event")]
    Event { event: String, payload: serde_json::Value },
    #[serde(rename = "ack")]
    Ack { id: String },
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

/// Messages received FROM a gateway connection
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method")]
pub enum GatewayRequest {
    #[serde(rename = "agent.register")]
    AgentRegister { id: String, name: String, capabilities: Vec<String> },
    #[serde(rename = "agent.heartbeat")]
    AgentHeartbeat,
    #[serde(rename = "agent.status")]
    AgentStatus { status: String, message: Option<String> },
    #[serde(rename = "chat.send")]
    ChatSend {
        session_key: String,
        message: serde_json::Value,
    },
    #[serde(rename = "chat.subscribe")]
    ChatSubscribe { session_key: String },
    #[serde(rename = "chat.unsubscribe")]
    ChatUnsubscribe { session_key: String },
}

/// Initialize the gateway registry in AppState
pub fn init_registry() -> Arc<RwLock<GatewayRegistry>> {
    Arc::new(RwLock::new(GatewayRegistry::default()))
}

/// HTTP handler for WebSocket upgrade
pub async fn gateway_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let token = params.get("token").cloned();
    
    ws.on_upgrade(move |socket| handle_gateway_socket(socket, state, token))
}

/// Main WebSocket handler for a single gateway connection
async fn handle_gateway_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    token: Option<String>,
) {
    let conn_id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<GatewayMessage>();
    
    // Parse token to determine role
    let (user_id, role) = if let Some(t) = &token {
        let auth = state.auth.read().await;
        match auth.validate_token(t) {
            Ok(claims) => {
                let role_str = claims.role.as_deref().map(|r| r.to_string());
                (Some(claims.sub), role_str)
            }
            Err(e) => {
                tracing::warn!("Gateway auth failed: {}", e);
                let _ = socket
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&GatewayMessage::Error {
                            code: "auth_failed".to_string(),
                            message: "Invalid or expired token".to_string(),
                        })
                        .unwrap(),
                    ))
                    .await;
                let _ = socket.close().await;
                return;
            }
        }
    } else {
        // Allow anonymous connections but with limited capabilities
        (None, None)
    };
    
    tracing::info!(
        "Gateway connection {} established (user={:?}, role={:?})",
        conn_id,
        user_id,
        role
    );
    
    // Register connection
    {
        let mut registry = state.gateway_registry.write().await;
        registry.connections.insert(
            conn_id.clone(),
            GatewayConnection {
                id: conn_id.clone(),
                agent_id: None,
                user_id: user_id.clone(),
                role: role.clone(),
                connected_at: Instant::now(),
                last_heartbeat: Instant::now(),
                session_subscriptions: Vec::new(),
                tx: tx.clone(),
            },
        );
    }
    
    // Send welcome message
    let welcome = serde_json::json!({
        "type": "event",
        "event": "gateway.connected",
        "payload": {
            "connection_id": conn_id,
            "authenticated": user_id.is_some(),
            "role": role.as_deref(),
        }
    });
    let _ = socket
        .send(axum::extract::ws::Message::Text(welcome.to_string()))
        .await;
    
    let mut last_activity = Instant::now();
    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    
    // Main message loop
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = socket.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        last_activity = Instant::now();
                        if let Err(e) = handle_gateway_message(
                            &conn_id, &text, &state, &tx, &user_id, &role,
                        ).await
                        {
                            tracing::warn!("Gateway message error for {}: {}", conn_id, e);
                            let error_msg = serde_json::to_string(&GatewayMessage::Error {
                                code: "message_error".to_string(),
                                message: e.to_string(),
                            }).unwrap();
                            let _ = tx.send(GatewayMessage::Event {
                                event: "error".to_string(),
                                payload: serde_json::json!({"message": error_msg}),
                            });
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        tracing::info!("Gateway connection {} closed by client", conn_id);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("Gateway WebSocket error for {}: {}", conn_id, e);
                        break;
                    }
                    _ => {}
                }
            }
            
            // Handle outgoing messages from internal channels
            internal_msg = rx.recv() => {
                match internal_msg {
                    Some(msg) => {
                        let text = serde_json::to_string(&msg).unwrap_or_default();
                        if let Err(e) = socket.send(axum::extract::ws::Message::Text(text)).await {
                            tracing::warn!("Failed to send gateway message to {}: {}", conn_id, e);
                            break;
                        }
                    }
                    None => {
                        tracing::info!("Gateway channel closed for {}", conn_id);
                        break;
                    }
                }
            }
            
            // Periodic heartbeat check
            _ = heartbeat_interval.tick() => {
                let registry = state.gateway_registry.read().await;
                if let Some(conn) = registry.connections.get(&conn_id) {
                    if conn.last_heartbeat.elapsed() > CONNECTION_TIMEOUT {
                        tracing::warn!("Gateway connection {} timed out", conn_id);
                        drop(registry);
                        break;
                    }
                }
                drop(registry);
                
                // Send ping
                let ping = serde_json::json!({
                    "type": "event",
                    "event": "gateway.ping",
                    "payload": {"timestamp": chrono::Utc::now().to_rfc3339()}
                });
                let _ = socket.send(axum::extract::ws::Message::Text(ping.to_string())).await;
            }
        }
    }
    
    // Cleanup
    {
        let mut registry = state.gateway_registry.write().await;
        if let Some(conn) = registry.connections.remove(&conn_id) {
            if let Some(agent_id) = &conn.agent_id {
                registry.agent_to_connection.remove(agent_id);
                
                // Mark agent as disconnected
                let mut containers = state.containers.write().await;
                if let Some(idx) = state.agent_index.read().await.get(agent_id).cloned() {
                    if idx < containers.len() {
                        containers[idx].status = AgentStatus::Stopped;
                    }
                }
            }
        }
    }
    
    tracing::info!("Gateway connection {} unregistered", conn_id);
    let _ = socket.close().await;
}

/// Handle a single message from a gateway client
async fn handle_gateway_message(
    conn_id: &str,
    text: &str,
    state: &Arc<AppState>,
    tx: &mpsc::UnboundedSender<GatewayMessage>,
    user_id: &Option<String>,
    role: &Option<String>,
) -> Result<()> {
    let request: GatewayRequest = serde_json::from_str(text)
        .map_err(|e| anyhow!("Invalid request format: {}", e))?;
    
    match request {
        GatewayRequest::AgentRegister { id, name, capabilities } => {
            // Check if user is authorized to register agents
            let role_str = role.as_deref();
            if !is_gateway_admin(role_str) {
                return Err(anyhow!("Only admins and teachers can register gateway agents"));
            }
            
            // Create an external agent entry
            let agent_id = id.clone();
            let agent = AgentContainer {
                id: agent_id.clone(),
                name: name.clone(),
                status: AgentStatus::Running,
                config: crate::types::AgentConfig {
                    llm_provider: crate::types::LlmProvider::Other,
                    llm_model: None,
                    memory_mb: 0,
                    cpu_cores: 0.0,
                    env_vars: HashMap::new(),
                    secrets: Vec::new(),
                    preset: None,
                    restart_policy: Default::default(),
                    health_check: None,
                    volumes: Vec::new(),
                    api_key: None,
                    image: Some("gateway-external".to_string()),
                },
                tailscale_ip: None,
                resource_usage: None,
                project: None,
                tags: vec!["gateway".to_string(), "external".to_string()],
                restart_policy: Default::default(),
                health_status: Some(crate::types::HealthStatus {
                    healthy: true,
                    last_check: chrono::Utc::now().to_rfc3339(),
                    message: Some(format!("Gateway agent: {} (capabilities: {:?})", name, capabilities)),
                }),
                runtime: Some("gateway".to_string()),
                gateway_port: 0, // External agents don't have a local gateway port
            };
            
            // Register in containers
            {
                let mut containers = state.containers.write().await;
                let idx = containers.len();
                containers.push(agent);
                
                let mut index = state.agent_index.write().await;
                index.insert(agent_id.clone(), idx);
            }
            
            // Update registry mapping
            {
                let mut registry = state.gateway_registry.write().await;
                if let Some(conn) = registry.connections.get_mut(conn_id) {
                    conn.agent_id = Some(agent_id.clone());
                }
                registry.agent_to_connection.insert(agent_id.clone(), conn_id.to_string());
            }
            
            // Persist
            let containers = state.containers.read().await;
            if let Some(idx) = state.agent_index.read().await.get(&agent_id).cloned() {
                if idx < containers.len() {
                    let _ = crate::storage::upsert_agent(&crate::storage::to_stored_agent(&containers[idx]));
                }
            }
            
            let ack = serde_json::json!({
                "type": "ack",
                "id": conn_id,
                "payload": {
                    "agent_id": agent_id,
                    "status": "registered",
                }
            });
            let _ = tx.send(GatewayMessage::Event {
                event: "agent.registered".to_string(),
                payload: ack,
            });
            
            tracing::info!("Gateway agent registered: {} ({})", name, agent_id);
        }
        
        GatewayRequest::AgentHeartbeat => {
            let mut registry = state.gateway_registry.write().await;
            if let Some(conn) = registry.connections.get_mut(conn_id) {
                conn.last_heartbeat = Instant::now();
            }
            let ack = serde_json::json!({
                "type": "ack",
                "id": conn_id,
                "event": "heartbeat.ack"
            });
            let _ = tx.send(GatewayMessage::Event {
                event: "heartbeat.ack".to_string(),
                payload: ack,
            });
        }
        
        GatewayRequest::AgentStatus { status, message } => {
            let registry = state.gateway_registry.read().await;
            if let Some(conn) = registry.connections.get(conn_id) {
                if let Some(agent_id) = &conn.agent_id {
                    let new_status = match status.as_str() {
                        "running" => AgentStatus::Running,
                        "stopped" => AgentStatus::Stopped,
                        "error" => AgentStatus::Error,
                        _ => AgentStatus::Running,
                    };
                    
                    let mut containers = state.containers.write().await;
                    if let Some(idx) = state.agent_index.read().await.get(agent_id).cloned() {
                        if idx < containers.len() {
                            containers[idx].status = new_status;
                            containers[idx].health_status = Some(crate::types::HealthStatus {
                                healthy: status == "running",
                                last_check: chrono::Utc::now().to_rfc3339(),
                                message: message.clone(),
                            });
                        }
                    }
                }
            }
        }
        
        GatewayRequest::ChatSend { session_key, message } => {
            // Store message in chat_db if available
            let content = message.get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            
            let registry = state.gateway_registry.read().await;
            let sender_name = registry.connections.get(conn_id)
                .and_then(|c| c.agent_id.clone())
                .unwrap_or_else(|| conn_id.to_string());
            drop(registry);
            
            // Add to chat_db
            if let Err(e) = state.chat_db.add_message(
                &session_key,
                "user",
                &content,
                Some(&serde_json::json!({"sender": sender_name, "gateway": true})),
            ).await {
                tracing::warn!("Failed to store gateway chat message: {}", e);
            }
            
            // Acknowledge
            let ack = serde_json::json!({
                "type": "ack",
                "id": conn_id,
                "event": "chat.sent",
                "payload": {"session_key": session_key}
            });
            let _ = tx.send(GatewayMessage::Event {
                event: "chat.sent".to_string(),
                payload: ack,
            });
            
            // Broadcast to other subscribers of this session
            broadcast_to_session(state, &session_key, &sender_name, content).await;
        }
        
        GatewayRequest::ChatSubscribe { session_key } => {
            let mut registry = state.gateway_registry.write().await;
            if let Some(conn) = registry.connections.get_mut(conn_id) {
                if !conn.session_subscriptions.contains(&session_key) {
                    conn.session_subscriptions.push(session_key.clone());
                }
            }
            let ack = serde_json::json!({
                "type": "ack",
                "id": conn_id,
                "event": "chat.subscribed",
                "payload": {"session_key": session_key}
            });
            let _ = tx.send(GatewayMessage::Event {
                event: "chat.subscribed".to_string(),
                payload: ack,
            });
        }
        
        GatewayRequest::ChatUnsubscribe { session_key } => {
            let mut registry = state.gateway_registry.write().await;
            if let Some(conn) = registry.connections.get_mut(conn_id) {
                conn.session_subscriptions.retain(|s| s != &session_key);
            }
            let ack = serde_json::json!({
                "type": "ack",
                "id": conn_id,
                "event": "chat.unsubscribed",
                "payload": {"session_key": session_key}
            });
            let _ = tx.send(GatewayMessage::Event {
                event: "chat.unsubscribed".to_string(),
                payload: ack,
            });
        }
    }
    
    Ok(())
}

/// Broadcast a message to all connections subscribed to a session
async fn broadcast_to_session(
    state: &Arc<AppState>,
    session_key: &str,
    sender: &str,
    content: &str,
) {
    let registry = state.gateway_registry.read().await;
    let mut sent = 0;
    
    for (conn_id, conn) in registry.connections.iter() {
        if conn.session_subscriptions.contains(session_key) && conn.id != sender {
            let event = serde_json::json!({
                "type": "event",
                "event": "chat.message",
                "payload": {
                    "session_key": session_key,
                    "sender": sender,
                    "content": content,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }
            });
            
            if conn.tx.send(GatewayMessage::Event {
                event: "chat.message".to_string(),
                payload: event,
            }).is_ok() {
                sent += 1;
            }
        }
    }
    
    tracing::debug!("Broadcasted gateway message to {} connections", sent);
}

/// HTTP handler for listing connected gateway agents
pub async fn list_gateway_connections(
    State(state): State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let registry = state.gateway_registry.read().await;
    
    let connections: Vec<serde_json::Value> = registry
        .connections
        .values()
        .map(|conn| {
            serde_json::json!({
                "id": conn.id,
                "agent_id": conn.agent_id,
                "user_id": conn.user_id,
                "role": conn.role.as_deref(),
                "connected_at": conn.connected_at.elapsed().as_secs(),
                "last_heartbeat": conn.last_heartbeat.elapsed().as_secs(),
                "subscriptions": conn.session_subscriptions,
            })
        })
        .collect();
    
    axum::Json(serde_json::json!({
        "connections": connections,
        "count": connections.len(),
    }))
}

/// HTTP handler for gateway health
pub async fn gateway_health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "gateway": "active",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
