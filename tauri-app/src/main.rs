// Claw Pen Desktop - Tauri App with Rust WebSockets
// With Ed25519 device identity and floating window support

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use futures_util::{SinkExt, StreamExt};
use http::request::Request;
use rand::rngs::OsRng;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::mpsc::{channel, Sender};
use tokio_tungstenite::connect_async_with_config;
use tokio_util::sync::CancellationToken;
use tungstenite::handshake::client::generate_key;

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub orchestrator_url: String,
    pub agent_gateway_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            orchestrator_url: "http://localhost:3001".to_string(),
            agent_gateway_url: "ws://127.0.0.1:18790/ws".to_string(),
        }
    }
}

pub struct AppState {
    pub ws_sender: Arc<tokio::sync::Mutex<Option<Sender<String>>>>,
    pub cancel_token: Arc<tokio::sync::Mutex<Option<CancellationToken>>>,
}

// Per-window state for floating windows
pub struct FloatingWindowState {
    pub agent_id: String,
    pub agent_name: String,
    pub port: u16,
    pub ws_sender: Arc<tokio::sync::Mutex<Option<Sender<String>>>>,
    pub cancel_token: Arc<tokio::sync::Mutex<Option<CancellationToken>>>,
}

pub struct WindowManager {
    pub floating_windows: Arc<tokio::sync::Mutex<HashMap<String, FloatingWindowState>>>,
}

fn get_device_keys_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".openclaw").join("claw-pen-device.json")
}

struct DeviceKeys {
    signing_key: SigningKey,
    device_id: String,
}

fn load_or_create_device_keys() -> Result<DeviceKeys> {
    let path = get_device_keys_path();

    if path.exists() {
        let data = fs::read_to_string(&path)?;
        let keys: serde_json::Value = serde_json::from_str(&data)?;

        let private_key_b64 = keys["privateKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing privateKey"))?;
        let private_key_bytes = BASE64.decode(private_key_b64)?;
        let bytes: [u8; 32] = private_key_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
        let signing_key = SigningKey::from_bytes(&bytes);

        let device_id = keys["deviceId"].as_str().unwrap_or("unknown").to_string();

        return Ok(DeviceKeys {
            signing_key,
            device_id,
        });
    }

    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let mut hasher = Sha256::new();
    hasher.update(verifying_key.to_bytes());
    let device_id = hex::encode(hasher.finalize());

    let keys_json = serde_json::json!({
        "privateKey": BASE64.encode(signing_key.to_bytes()),
        "publicKey": BASE64.encode(verifying_key.to_bytes()),
        "deviceId": device_id
    });

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&keys_json)?)?;

    Ok(DeviceKeys {
        signing_key,
        device_id,
    })
}

#[tauri::command]
async fn get_config() -> Result<AppConfig, String> {
    Ok(AppConfig::default())
}

fn build_connect_request(req_id: &str, nonce: &str, device_keys: &DeviceKeys) -> String {
    let signed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let scopes = "operator.admin,operator.approvals,operator.pairing";

    let message = format!(
        "v2|{}|openclaw-control-ui|webchat|operator|{}|{}||{}",
        device_keys.device_id, scopes, signed_at, nonce
    );

    eprintln!("[Device] Signing message: {}", &message);

    let signature = device_keys.signing_key.sign(message.as_bytes());
    let signature_b64 = BASE64.encode(signature.to_bytes());
    let public_key_b64 = BASE64.encode(device_keys.signing_key.verifying_key().to_bytes());

    serde_json::json!({
        "type": "req",
        "id": req_id,
        "method": "connect",
        "params": {
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "openclaw-control-ui",
                "version": "1.0.0",
                "platform": "desktop",
                "mode": "webchat"
            },
            "role": "operator",
            "scopes": ["operator.admin", "operator.approvals", "operator.pairing"],
            "device": {
                "id": device_keys.device_id,
                "publicKey": public_key_b64,
                "signature": signature_b64,
                "signedAt": signed_at,
                "nonce": nonce
            },
            "caps": [],
            "commands": []
        }
    })
    .to_string()
}

#[tauri::command]
async fn connect_websocket(
    app: AppHandle,
    _state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    // Simplified: just emit success for HTTP API mode
    // The frontend handles all communication via HTTP fetch
    eprintln!("[HTTP] Using HTTP API mode, orchestrator at: {}", url);
    let _ = app.emit("ws-authenticated", true);
    Ok(())
}

fn extract_nonce(json: &str) -> Option<&str> {
    if let Some(start) = json.find("\"nonce\":\"") {
        let start = start + 9;
        if let Some(end) = json[start..].find("\"") {
            return Some(&json[start..start + end]);
        }
    }
    None
}

fn uuid() -> String {
    let mut rng = rand::thread_rng();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        rng.gen::<u32>(),
        rng.gen::<u16>(),
        rng.gen::<u16>(),
        rng.gen::<u16>(),
        rng.gen::<u64>() & 0xffffffffffff
    )
}

#[tauri::command]
async fn send_chat_message(state: State<'_, AppState>, text: String) -> Result<(), String> {
    let sender = state.ws_sender.lock().await;

    if let Some(tx) = sender.as_ref() {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let idempotency_key = uuid();
        let msg = serde_json::json!({
            "type": "req",
            "id": format!("msg-{}", id),
            "method": "chat.send",
            "params": {
                "sessionKey": "main",
                "message": text,
                "deliver": false,
                "idempotencyKey": idempotency_key
            }
        })
        .to_string();

        tx.send(msg)
            .await
            .map_err(|e: tokio::sync::mpsc::error::SendError<String>| e.to_string())?;
        Ok(())
    } else {
        Err("WebSocket not connected".to_string())
    }
}

#[tauri::command]
async fn pop_out_agent(
    app: AppHandle,
    window_manager: State<'_, WindowManager>,
    agent_id: String,
    agent_name: String,
    port: u16,
) -> Result<String, String> {
    let window_label = format!("agent-{}", agent_id);

    // Check if window already exists
    if app.get_webview_window(&window_label).is_some() {
        // Focus existing window
        if let Some(win) = app.get_webview_window(&window_label) {
            win.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(window_label);
    }

    eprintln!(
        "[PopOut] Creating floating window for {} on port {}",
        agent_name, port
    );

    // Create floating window
    let url = format!(
        "/dist/index.html?floating=1&agent_id={}&agent_name={}&port={}",
        agent_id,
        urlencoding::encode(&agent_name),
        port
    );

    WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::App(url.into()))
        .title(format!("{} - Claw Pen", agent_name))
        .inner_size(600.0, 700.0)
        .min_inner_size(400.0, 500.0)
        .build()
        .map_err(|e| e.to_string())?;

    // Store state for this window
    let state = FloatingWindowState {
        agent_id: agent_id.clone(),
        agent_name: agent_name.clone(),
        port,
        ws_sender: Arc::new(tokio::sync::Mutex::new(None)),
        cancel_token: Arc::new(tokio::sync::Mutex::new(None)),
    };

    window_manager
        .floating_windows
        .lock()
        .await
        .insert(window_label.clone(), state);

    Ok(window_label)
}

#[tauri::command]
async fn connect_floating_window(
    app: AppHandle,
    window_manager: State<'_, WindowManager>,
    window_label: String,
) -> Result<(), String> {
    // Extract the values we need from state before spawning
    let (port, agent_name) = {
        let mut windows = window_manager.floating_windows.lock().await;
        let state = windows
            .get_mut(&window_label)
            .ok_or_else(|| format!("Window {} not found", window_label))?;

        // Cancel existing connection if any
        if let Some(token) = state.cancel_token.lock().await.take() {
            token.cancel();
        }

        (state.port, state.agent_name.clone())
    };

    let url = format!("ws://localhost:{}/ws", port);

    // Get sender from state
    let ws_sender = {
        let windows = window_manager.floating_windows.lock().await;
        let state = windows.get(&window_label).unwrap();
        state.ws_sender.clone()
    };

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    // Store cancel token
    {
        let windows = window_manager.floating_windows.lock().await;
        let state = windows.get(&window_label).unwrap();
        *state.cancel_token.lock().await = Some(cancel_token);
    }

    let (tx, mut rx) = channel::<String>(100);
    *ws_sender.lock().await = Some(tx);

    let app_handle = app.clone();
    let window_label_clone = window_label.clone();

    let device_keys =
        load_or_create_device_keys().map_err(|e| format!("Failed to load device keys: {}", e))?;
    let signing_key_bytes = device_keys.signing_key.to_bytes();
    let device_id = device_keys.device_id;

    tokio::spawn(async move {
        let request = Request::builder()
            .uri(&url)
            .header("Host", format!("localhost:{}", port))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key())
            .header("Origin", format!("http://localhost:{}", port))
            .body(())
            .unwrap();

        match connect_async_with_config(request, None, false).await {
            Ok((ws_stream, _)) => {
                eprintln!("[Floating:{}] Connected", agent_name);
                let _ = app_handle.emit_to(&window_label_clone, "ws-connected", true);

                let (mut write, mut read) = ws_stream.split();
                let mut authenticated = false;
                let mut connect_sent = false;

                let signing_key = SigningKey::from_bytes(&signing_key_bytes);
                let dk = DeviceKeys {
                    signing_key,
                    device_id,
                };

                loop {
                    tokio::select! {
                        _ = cancel_clone.cancelled() => break,

                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)), if !connect_sent && !authenticated => {
                            authenticated = true;
                            connect_sent = true;
                            let _ = app_handle.emit_to(&window_label_clone, "ws-authenticated", true);
                        }

                        msg = read.next() => {
                            match msg {
                                Some(Ok(m)) if m.is_text() => {
                                    let text = m.to_string();
                                    if !connect_sent && text.contains("\"event\":\"connect.challenge\"") {
                                        let nonce = extract_nonce(&text).unwrap_or("");
                                        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                                        let response = build_connect_request(&format!("fp-{}", id), nonce, &dk);
                                        let _ = write.send(tungstenite::Message::Text(response)).await;
                                        connect_sent = true;
                                    } else if text.contains("\"ok\":true") && text.contains("\"id\":\"fp-") {
                                        authenticated = true;
                                        let _ = app_handle.emit_to(&window_label_clone, "ws-authenticated", true);
                                    } else if authenticated {
                                        let _ = app_handle.emit_to(&window_label_clone, "ws-message", &text);
                                    }
                                }
                                Some(Ok(_)) | None => break,
                                Some(Err(_)) => break,
                            }
                        }

                        msg = rx.recv() => {
                            if let Some(text) = msg {
                                if authenticated {
                                    let _ = write.send(tungstenite::Message::Text(text)).await;
                                }
                            }
                        }
                    }
                }
                let _ = app_handle.emit_to(&window_label_clone, "ws-connected", false);
            }
            Err(e) => {
                eprintln!("[Floating:{}] Connection failed: {}", agent_name, e);
                let _ = app_handle.emit_to(
                    &window_label_clone,
                    "ws-error",
                    format!("Connection failed: {}", e),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn send_floating_message(
    window_manager: State<'_, WindowManager>,
    window_label: String,
    text: String,
) -> Result<(), String> {
    // Get sender reference from state
    let ws_sender = {
        let windows = window_manager.floating_windows.lock().await;
        let state = windows
            .get(&window_label)
            .ok_or_else(|| format!("Window {} not found", window_label))?;
        state.ws_sender.clone()
    };

    let sender = ws_sender.lock().await;
    if let Some(tx) = sender.as_ref() {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let msg = serde_json::json!({
            "type": "req",
            "id": format!("msg-{}", id),
            "method": "chat.send",
            "params": {
                "sessionKey": "main",
                "message": text,
                "deliver": false,
                "idempotencyKey": uuid()
            }
        })
        .to_string();

        tx.send(msg).await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("WebSocket not connected".to_string())
    }
}

#[tauri::command]
async fn launch_shell(container_id: String, _container_name: String) -> Result<(), String> {
    use std::process::Command;

    let docker_command = format!("docker exec -it {} /bin/bash", container_id);

    #[cfg(target_os = "windows")]
    {
        // On Windows, use cmd /c start to launch PowerShell in a new window
        Command::new("cmd")
            .args([
                "/c",
                "start",
                "powershell.exe",
                "-NoExit",
                "-Command",
                &docker_command,
            ])
            .spawn()
            .map_err(|e| format!("Failed to launch PowerShell: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On Unix-like systems, launch the default terminal with the docker command
        #[cfg(target_os = "macos")]
        let terminal_cmd = "osascript";
        #[cfg(target_os = "macos")]
        let terminal_args = [
            "-e",
            &format!("tell application \"Terminal\" to do script \"{}\"", docker_command.replace("\"", "\\\\\"")),
        ];

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        let terminal_cmd = "gnome-terminal";
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        let terminal_args = ["--", "sh", "-c", &docker_command];

        Command::new(terminal_cmd)
            .args(&terminal_args)
            .spawn()
            .map_err(|e| format!("Failed to launch terminal: {}", e))?;
    }

    Ok(())
}

// URL encoding for query params
mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}

fn main() {
    let state = AppState {
        ws_sender: Arc::new(tokio::sync::Mutex::new(None)),
        cancel_token: Arc::new(tokio::sync::Mutex::new(None)),
    };

    let window_manager = WindowManager {
        floating_windows: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .manage(state)
        .manage(window_manager)
        .invoke_handler(tauri::generate_handler![
            get_config,
            connect_websocket,
            send_chat_message,
            pop_out_agent,
            connect_floating_window,
            send_floating_message,
            launch_shell,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
