// Tauri JS API bindings for Yew/WASM

use js_sys::{Function, Object, Promise, Reflect};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::window;

/// Check if running in Tauri environment
pub fn is_tauri() -> bool {
    if let Some(win) = window() {
        Reflect::has(&win, &JsValue::from_str("__TAURI__")).unwrap_or(false)
    } else {
        false
    }
}

/// Get the Tauri object from window
fn get_tauri() -> Option<Object> {
    let win = window()?;
    let tauri = Reflect::get(&win, &JsValue::from_str("__TAURI__")).ok()?;
    tauri.dyn_into::<Object>().ok()
}

/// Get the event module from Tauri
fn get_tauri_event() -> Option<Object> {
    let tauri = get_tauri()?;
    let event = Reflect::get(&tauri, &JsValue::from_str("event")).ok()?;
    event.dyn_into::<Object>().ok()
}

/// Get the core module from Tauri
fn get_tauri_core() -> Option<Object> {
    let tauri = get_tauri()?;
    let core = Reflect::get(&tauri, &JsValue::from_str("core")).ok()?;
    core.dyn_into::<Object>().ok()
}

/// Invoke a Tauri command
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: Option<impl Serialize>) -> Result<T, String> {
    let core = get_tauri_core().ok_or("Tauri core not available")?;
    let invoke_fn: Function = Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| format!("Failed to get invoke: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("invoke is not a function: {:?}", e))?;

    let cmd_js = JsValue::from_str(cmd);
    let args_js = match args {
        Some(a) => serde_wasm_bindgen::to_value(&a).map_err(|e| format!("Serialize error: {}", e))?,
        None => JsValue::UNDEFINED,
    };

    let promise: Promise = invoke_fn
        .call2(&JsValue::NULL, &cmd_js, &args_js)
        .map_err(|e| format!("Invoke call failed: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("Result is not a promise: {:?}", e))?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| format!("Invoke async error: {:?}", e))?;

    serde_wasm_bindgen::from_value(result).map_err(|e| format!("Deserialize error: {}", e))
}

/// Invoke a Tauri command without return value
pub async fn invoke_void(cmd: &str, args: Option<impl Serialize>) -> Result<(), String> {
    let core = get_tauri_core().ok_or("Tauri core not available")?;
    let invoke_fn: Function = Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| format!("Failed to get invoke: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("invoke is not a function: {:?}", e))?;

    let cmd_js = JsValue::from_str(cmd);
    let args_js = match args {
        Some(a) => serde_wasm_bindgen::to_value(&a).map_err(|e| format!("Serialize error: {}", e))?,
        None => JsValue::UNDEFINED,
    };

    let promise: Promise = invoke_fn
        .call2(&JsValue::NULL, &cmd_js, &args_js)
        .map_err(|e| format!("Invoke call failed: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("Result is not a promise: {:?}", e))?;

    JsFuture::from(promise)
        .await
        .map_err(|e| format!("Invoke async error: {:?}", e))?;

    Ok(())
}

// === Typed API wrappers ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub orchestrator_url: String,
    pub agent_gateway_url: String,
}

/// Get app configuration
pub async fn get_config() -> Result<AppConfig, String> {
    invoke::<AppConfig>("get_config", None::<()>).await
}

/// Connect to WebSocket
pub async fn connect_websocket(url: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        url: String,
    }
    invoke_void("connect_websocket", Some(Args { url: url.to_string() })).await
}

/// Send a chat message
pub async fn send_chat_message(text: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        text: String,
    }
    invoke_void("send_chat_message", Some(Args { text: text.to_string() })).await
}

/// Pop out an agent into a floating window
pub async fn pop_out_agent(agent_id: &str, agent_name: &str, port: u16) -> Result<String, String> {
    #[derive(Serialize)]
    struct Args {
        agent_id: String,
        agent_name: String,
        port: u16,
    }
    invoke::<String>(
        "pop_out_agent",
        Some(Args {
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            port,
        }),
    )
    .await
}

/// Connect a floating window to its agent
pub async fn connect_floating_window(window_label: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        window_label: String,
    }
    invoke_void(
        "connect_floating_window",
        Some(Args {
            window_label: window_label.to_string(),
        }),
    )
    .await
}

/// Send message from floating window
pub async fn send_floating_message(window_label: &str, text: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        window_label: String,
        text: String,
    }
    invoke_void(
        "send_floating_message",
        Some(Args {
            window_label: window_label.to_string(),
            text: text.to_string(),
        }),
    )
    .await
}

// === Event helpers ===

/// Set up a Tauri event listener
/// Returns an unlisten function on success
pub async fn listen_event<F>(event: &str, callback: F) -> Result<Box<dyn FnOnce()>, String>
where
    F: FnMut(serde_json::Value) + 'static,
{
    let event_module = get_tauri_event().ok_or("Tauri event module not available")?;
    let listen_fn: Function = Reflect::get(&event_module, &JsValue::from_str("listen"))
        .map_err(|e| format!("Failed to get listen: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("listen is not a function: {:?}", e))?;

    // Wrap callback in Rc<RefCell> for interior mutability
    let callback = Rc::new(RefCell::new(callback));

    let closure = Closure::wrap(Box::new(move |event_obj: JsValue| {
        // The event object from Tauri has a `payload` field
        if let Ok(payload) = Reflect::get(&event_obj, &JsValue::from_str("payload")) {
            if let Ok(value) = serde_wasm_bindgen::from_value::<serde_json::Value>(payload.clone()) {
                if let Ok(mut cb) = callback.try_borrow_mut() {
                    cb(value);
                }
            } else if let Some(s) = payload.as_string() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&s) {
                    if let Ok(mut cb) = callback.try_borrow_mut() {
                        cb(value);
                    }
                } else {
                    // Just pass as string
                    if let Ok(mut cb) = callback.try_borrow_mut() {
                        cb(serde_json::Value::String(s));
                    }
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    let event_name = JsValue::from_str(event);
    let promise: Promise = listen_fn
        .call2(&JsValue::NULL, &event_name, closure.as_ref().unchecked_ref())
        .map_err(|e| format!("listen call failed: {:?}", e))?
        .dyn_into()
        .map_err(|e| format!("Result is not a promise: {:?}", e))?;

    let unlisten_fn = JsFuture::from(promise)
        .await
        .map_err(|e| format!("listen async error: {:?}", e))?;

    // Forget the closure - it will live for the lifetime of the page
    // The unlisten_fn can still be called to remove the listener
    closure.forget();

    Ok(Box::new(move || {
        // Call unlisten to remove the listener
        if let Some(fn_ref) = unlisten_fn.dyn_ref::<Function>() {
            let _ = fn_ref.call0(&JsValue::NULL);
        }
    }))
}
