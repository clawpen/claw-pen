// RPC (Remote Procedure Call) system for agent-to-agent communication
// Uses WebSocket connections for reliable message passing

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc as StdArc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{tungstenite::Message as TungsteniteMessage, WebSocketStream};
use futures_util::{SinkExt, StreamExt};

use crate::types::{AgentContainer, AgentMessage, DirectMessage, RequestMessage, ResponseMessage};

/// Response waiting for a specific request
struct PendingResponse {
    response_tx: mpsc::Sender<AgentMessage>,
    timeout: std::time::Instant,
}

// Type alias for the WebSocket stream type to avoid complex generic nesting
type WsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// RPC client for managing agent-to-agent connections
pub struct RpcClient {
    // Active WebSocket connections to agents
    connections: RwLock<HashMap<String, StdArc<Mutex<WsStream>>>>,
    // Pending responses waiting for replies
    pending_responses: StdArc<RwLock<HashMap<String, PendingResponse>>>,
}

impl RpcClient {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            pending_responses: StdArc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Send a message from one agent to another
    pub async fn send_message(
        &self,
        from_agent: &AgentContainer,
        to_agent: &AgentContainer,
        message: &AgentMessage,
    ) -> Result<AgentMessage> {
        // Get recipient's Tailscale IP
        let to_ip = to_agent
            .tailscale_ip
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Recipient agent {} has no Tailscale IP", to_agent.name))?;

        // Check if connection exists or create a new one
        let connection = self
            .get_or_create_connection(to_agent.id.clone(), to_ip, Some(to_agent.gateway_port))
            .await?;

        // Serialize the message
        let message_json = serde_json::to_string(message)
            .map_err(|e| anyhow::anyhow!("Failed to serialize message: {}", e))?;

        // Send the message via WebSocket
        connection
            .lock()
            .await
            .send(TungsteniteMessage::Text(message_json))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;

        // If this is a Request message, wait for a Response
        if let AgentMessage::Request(req) = message {
            // Wait for response with matching request_id
            self.wait_for_response(&req.id, std::time::Duration::from_secs(req.timeout))
                .await
        } else {
            // For direct messages, return an acknowledgment
            Ok(AgentMessage::Direct(DirectMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: from_agent.id.clone(),
                to: to_agent.id.clone(),
                content: "Message delivered".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                metadata: HashMap::new(),
            }))
        }
    }

    /// Get or create a WebSocket connection to an agent
    async fn get_or_create_connection(
        &self,
        agent_id: String,
        ip: &str,
        port: Option<u16>,
    ) -> Result<std::sync::Arc<Mutex<WsStream>>> {
        // Check if connection already exists
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&agent_id) {
                return Ok(conn.clone());
            }
        }

        // Create new connection
        let gateway_port = port.unwrap_or(18792);
        let url = format!("ws://{}:{}/gateway", ip, gateway_port);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to agent {} at {}: {}",
                agent_id,
                url,
                e
            )
        })?;

        // Store the connection
        let conn = std::sync::Arc::new(Mutex::new(ws_stream));

        let mut connections = self.connections.write().await;
        connections.insert(agent_id.clone(), conn.clone());

        // Spawn task to listen for responses from this agent
        let conn_clone = StdArc::clone(&conn);
        let agent_id_clone = agent_id.clone();
        let pending_responses = StdArc::clone(&self.pending_responses);
        tokio::spawn(async move {
            if let Err(e) =
                Self::listen_for_responses(conn_clone, agent_id_clone, pending_responses).await
            {
                tracing::error!("Error listening for responses from {}: {}", agent_id, e);
            }
        });

        Ok(conn)
    }

    /// Listen for response messages from an agent
    async fn listen_for_responses(
        conn: StdArc<Mutex<WsStream>>,
        agent_id: String,
        pending_responses: StdArc<RwLock<HashMap<String, PendingResponse>>>,
    ) -> Result<()> {
        use futures_util::StreamExt;

        let mut recv = conn.lock().await;

        // Listen for incoming messages
        while let Some(result) = recv.next().await {
            match result {
                Ok(TungsteniteMessage::Text(text)) => {
                    tracing::info!("Received message from agent {}: {}", agent_id, text);

                    // Parse the message
                    if let Ok(msg) = serde_json::from_str::<AgentMessage>(&text) {
                        match msg {
                            AgentMessage::Response(resp) => {
                                let request_id = resp.request_id.clone();
                                // Find the waiting request handler
                                let mut handlers = pending_responses.write().await;
                                if let Some(pending) = handlers.remove(&request_id) {
                                    // Send the response to the waiting task
                                    let _ = pending
                                        .response_tx
                                        .send(AgentMessage::Response(resp))
                                        .await;
                                    tracing::debug!(
                                        "Delivered response for request {}",
                                        request_id
                                    );
                                } else {
                                    tracing::warn!(
                                        "No pending handler for request {}",
                                        request_id
                                    );
                                }
                            }
                            _ => {
                                tracing::debug!(
                                    "Received non-response message from agent {}",
                                    agent_id
                                );
                            }
                        }
                    }
                }
                Ok(TungsteniteMessage::Close(_)) => {
                    tracing::info!("Agent {} closed connection", agent_id);
                    break;
                }
                Err(e) => {
                    tracing::error!("Error receiving from agent {}: {}", agent_id, e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Wait for a response to a request
    async fn wait_for_response(
        &self,
        request_id: &str,
        timeout: std::time::Duration,
    ) -> Result<AgentMessage> {
        let start = std::time::Instant::now();

        // Create a channel for the response
        let (response_tx, mut response_rx) = mpsc::channel(1);

        // Register the pending response
        {
            let mut handlers = self.pending_responses.write().await;
            handlers.insert(
                request_id.to_string(),
                PendingResponse {
                    response_tx,
                    timeout: start + timeout,
                },
            );
        }

        // Wait for the response or timeout
        while start.elapsed() < timeout {
            tokio::select! {
                result = response_rx.recv() => {
                    if let Some(response) = result {
                        return Ok(response);
                    } else {
                        return Err(anyhow::anyhow!("Response channel closed"));
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Check timeout on next iteration
                }
            }
        }

        // Clean up the pending response
        {
            let mut handlers = self.pending_responses.write().await;
            handlers.remove(request_id);
        }

        Err(anyhow::anyhow!(
            "Request {} timed out after {:?}",
            request_id,
            timeout
        ))
    }
}

/// Create a new RPC client
pub fn create_rpc_client() -> RpcClient {
    RpcClient::new()
}
