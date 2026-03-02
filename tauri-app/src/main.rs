// Claw Pen Desktop - Tauri App with Rust WebSockets
// With Ed25519 device identity

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
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc::{channel, Sender};
use tokio_tungstenite::connect_async_with_config;
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
            orchestrator_url: "http://localhost:3000".to_string(),
            agent_gateway_url: "ws://127.0.0.1:18790/ws".to_string(),
        }
    }
}

pub struct AppState {
    pub ws_sender: Arc<tokio::sync::Mutex<Option<Sender<String>>>>,
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
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    let app_handle = app.clone();

    let device_keys =
        load_or_create_device_keys().map_err(|e| format!("Failed to load device keys: {}", e))?;
    eprintln!("[Device] ID: {}", device_keys.device_id);

    let (tx, mut rx) = channel::<String>(100);
    *state.ws_sender.lock().await = Some(tx);

    eprintln!("[WS] Connecting to: {}", url);

    let signing_key_bytes = device_keys.signing_key.to_bytes();
    let device_id = device_keys.device_id.clone();

    tokio::spawn(async move {
        loop {
            eprintln!("[WS] Attempting connection to {}", url);

            let request = Request::builder()
                .uri(&url)
                .header("Host", "127.0.0.1:18790")
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", generate_key())
                .header("Origin", "http://127.0.0.1:18790")
                .body(())
                .unwrap();

            match connect_async_with_config(request, None, false).await {
                Ok((ws_stream, _)) => {
                    eprintln!("[WS] Connected successfully");
                    let _ = app_handle.emit("ws-connected", true);

                    let (mut write, mut read) = ws_stream.split();
                    let mut authenticated = false;
                    let mut connect_sent = false;

                    let signing_key = SigningKey::from_bytes(&signing_key_bytes);
                    let dk = DeviceKeys {
                        signing_key,
                        device_id: device_id.clone(),
                    };

                    loop {
                        tokio::select! {
                            // Timeout for no-auth mode: if no challenge after 2s, assume auth disabled
                            _ = tokio::time::sleep(std::time::Duration::from_secs(2)), if !connect_sent && !authenticated => {
                                eprintln!("[WS] No challenge received - assuming no-auth mode");
                                authenticated = true;
                                connect_sent = true;
                                let _ = app_handle.emit("ws-authenticated", true);
                            }
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(m)) => {
                                        if m.is_text() {
                                            let text = m.to_string();

                                            if !connect_sent && text.contains("\"event\":\"connect.challenge\"") {
                                                let nonce = extract_nonce(&text).unwrap_or("");
                                                eprintln!("[WS] Got challenge, nonce: {}", nonce);

                                                let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                                                let response = build_connect_request(
                                                    &format!("cp-{}", id),
                                                    nonce,
                                                    &dk
                                                );
                                                eprintln!("[WS] Sending connect");
                                                if let Err(e) = write.send(tungstenite::Message::Text(response)).await {
                                                    eprintln!("[WS] Send error: {}", e);
                                                    break;
                                                }
                                                connect_sent = true;
                                            } else if text.contains("\"ok\":true") && text.contains("\"id\":\"cp-") {
                                                eprintln!("[WS] Authenticated!");
                                                authenticated = true;
                                                let _ = app_handle.emit("ws-authenticated", true);
                                            } else if text.contains("\"error\"") {
                                                eprintln!("[WS] Error: {}", &text[..text.len().min(200)]);
                                                let _ = app_handle.emit("ws-error", &text);
                                            } else if authenticated {
                                                eprintln!("[WS] Event: {}", &text[..text.len().min(100)]);
                                                let _ = app_handle.emit("ws-message", &text);
                                            }
                                        } else if m.is_close() {
                                            eprintln!("[WS] Server closed");
                                            break;
                                        }
                                    }
                                    Some(Err(e)) => {
                                        eprintln!("[WS] Read error: {}", e);
                                        break;
                                    }
                                    None => break,
                                }
                            }
                            msg = rx.recv() => {
                                if let Some(text) = msg {
                                    if authenticated {
                                        eprintln!("[WS] TX: {}", &text);
                                        if let Err(e) = write.send(tungstenite::Message::Text(text)).await {
                                            eprintln!("[WS] Send error: {}", e);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let _ = app_handle.emit("ws-connected", false);
                }
                Err(e) => {
                    eprintln!("[WS] Connection failed: {}", e);
                    let _ = app_handle.emit("ws-connected", false);
                }
            }

            eprintln!("[WS] Reconnecting in 3s...");
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    });

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
            connect_websocket,
            send_chat_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
