// Claw Pen Desktop - Tauri App with Rust WebSockets
// Bidirectional WebSocket communication

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc::{channel, Sender};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub config: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm_provider: String,
    pub llm_model: Option<String>,
    pub memory_mb: u32,
    pub cpu_cores: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub orchestrator_url: String,
    pub agent_gateway_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            orchestrator_url: "http://localhost:3000".to_string(),
            agent_gateway_url: "ws://127.0.0.1:18790/ws".to_string(),  // Use 127.0.0.1
        }
    }
}

// App state to hold the WebSocket sender
pub struct AppState {
    pub ws_sender: Arc<tokio::sync::Mutex<Option<Sender<String>>>>,
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
async fn get_config() -> Result<AppConfig, String> {
    Ok(AppConfig::default())
}

#[tauri::command]
async fn health_check() -> Result<String, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:3000/health")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("Orchestrator is running".to_string())
    } else {
        Err("Orchestrator not responding".to_string())
    }
}

#[tauri::command]
async fn list_agents() -> Result<Vec<AgentContainer>, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:3000/api/agents")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    response.json().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_agent(id: String) -> Result<AgentContainer, String> {
    let client = reqwest::Client::new();
    client
        .post(format!("http://localhost:3000/api/agents/{}/start", id))
        .json(&())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_agent(id: String) -> Result<AgentContainer, String> {
    let client = reqwest::Client::new();
    client
        .post(format!("http://localhost:3000/api/agents/{}/stop", id))
        .json(&())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn connect_websocket(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    
    // Create channel for outgoing messages
    let (tx, mut rx) = channel::<String>(100);
    
    // Store the sender in state
    *state.ws_sender.lock().await = Some(tx.clone());
    
    eprintln!("[WS] Connecting to: {}", url);

    tokio::spawn(async move {
        loop {
            eprintln!("[WS] Attempting connection to {}", url);
            
            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    eprintln!("[WS] Connected successfully");
                    let _ = app_handle.emit("ws-connected", true);
                    
                    let (mut write, mut read) = ws_stream.split();
                    
                    // Clone for the send task
                    let rx_ref = &mut rx;
                    
                    // Task for receiving messages
                    loop {
                        tokio::select! {
                            // Receive from WebSocket
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(m)) => {
                                        if m.is_text() {
                                            let text = m.to_string();
                                            eprintln!("[WS] Received: {} bytes", text.len());
                                            let _ = app_handle.emit("ws-message", &text);
                                        } else if m.is_close() {
                                            eprintln!("[WS] Server closed connection");
                                            let _ = app_handle.emit("ws-connected", false);
                                            break;
                                        }
                                    }
                                    Some(Err(e)) => {
                                        eprintln!("[WS] Read error: {}", e);
                                        break;
                                    }
                                    None => {
                                        eprintln!("[WS] Stream ended");
                                        break;
                                    }
                                }
                            }
                            // Send to WebSocket
                            msg = rx_ref.recv() => {
                                match msg {
                                    Some(text) => {
                                        eprintln!("[WS] Sending: {} bytes", text.len());
                                        if let Err(e) = write.send(WsMessage::Text(text)).await {
                                            eprintln!("[WS] Send error: {}", e);
                                            break;
                                        }
                                    }
                                    None => {
                                        eprintln!("[WS] Channel closed");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    eprintln!("[WS] Disconnected");
                    let _ = app_handle.emit("ws-connected", false);
                }
                Err(e) => {
                    eprintln!("[WS] Connection failed: {}", e);
                    let _ = app_handle.emit("ws-connected", false);
                }
            }
            
            // Reconnect after 3 seconds
            eprintln!("[WS] Reconnecting in 3 seconds...");
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    });

    Ok(())
}

#[tauri::command]
async fn send_chat_message(
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    let sender = state.ws_sender.lock().await;
    
    if let Some(tx) = sender.as_ref() {
        // Send as chat message format
        let msg = serde_json::json!({
            "type": "chat.send",
            "text": text
        }).to_string();
        
        tx.send(msg).await.map_err(|e| e.to_string())?;
        eprintln!("[WS] Queued message for send");
        Ok(())
    } else {
        Err("WebSocket not connected".to_string())
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let state = AppState {
        ws_sender: Arc::new(tokio::sync::Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_config,
            health_check,
            list_agents,
            start_agent,
            stop_agent,
            connect_websocket,
            send_chat_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
