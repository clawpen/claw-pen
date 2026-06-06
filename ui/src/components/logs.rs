use crate::api::get_token;
use gloo_net::websocket::Message as WsMessage;
use gloo_net::websocket::futures::WebSocket;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use futures::StreamExt;

#[derive(Properties, PartialEq)]
pub struct LogsPanelProps {
    pub agent_id: String,
    pub agent_name: String,
    pub on_close: Callback<()>,
}

#[function_component(LogsPanel)]
pub fn logs_panel(props: &LogsPanelProps) -> Html {
    let logs = use_state(Vec::<String>::new);
    let connected = use_state(|| false);
    let error = use_state(|| None::<String>);

    {
        let agent_id = props.agent_id.clone();
        let logs = logs.clone();
        let connected = connected.clone();
        let error = error.clone();

        use_effect_with(agent_id.clone(), move |_| {
            let token = get_token().unwrap_or_default();
            let window = web_sys::window().unwrap();
            let protocol = window.location().protocol().unwrap_or_else(|_| "http:".to_string());
            let ws_protocol = if protocol.starts_with("https") { "wss" } else { "ws" };
            let host = window.location().host().unwrap_or_else(|_| "localhost:3001".to_string());
            let ws_url = format!(
                "{}://{}/api/agents/{}/logs/stream?token={}",
                ws_protocol,
                host,
                agent_id,
                token
            );

            match WebSocket::open(&ws_url) {
                Ok(ws) => {
                    connected.set(true);
                    error.set(None);

                    spawn_local(async move {
                        let mut ws = ws;
                        while let Some(msg) = ws.next().await {
                            match msg {
                                Ok(WsMessage::Text(text)) => {
                                    let mut current = (*logs).clone();
                                    current.push(text);
                                    if current.len() > 1000 {
                                        current = current.split_off(current.len() - 1000);
                                    }
                                    logs.set(current);
                                }
                                Ok(WsMessage::Bytes(_)) => {}
                                Err(e) => {
                                    web_sys::console::error_1(&format!("WebSocket error: {}", e).into());
                                    break;
                                }
                            }
                        }
                        connected.set(false);
                    });
                }
                Err(e) => {
                    error.set(Some(format!("Failed to connect: {}", e)));
                    connected.set(false);
                }
            }

            || {}
        });
    }

    let on_close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_e: MouseEvent| {
            on_close.emit(());
        })
    };

    let on_clear = {
        let logs = logs.clone();
        Callback::from(move |_| {
            logs.set(Vec::new());
        })
    };

    let log_text = logs.join("\n");

    html! {
        <div class="logs-panel">
            <div class="logs-header">
                <div class="logs-title">
                    <span class="logs-icon">{"📋"}</span>
                    <span>{format!("Logs: {}", props.agent_name)}</span>
                    <span class={format!("status-indicator {}", if *connected { "connected" } else { "disconnected" })}
                    >
                        {if *connected { "● Live" } else { "○ Disconnected" }}
                    </span>
                </div>
                <div class="logs-actions">
                    <button class="btn-clear" onclick={on_clear}>{"Clear"}</button>
                    <button class="btn-close" onclick={on_close_click}>{"×"}</button>
                </div>
            </div>
            if let Some(ref err) = *error {
                <div class="logs-error">{err}</div>
            }
            <div class="logs-content">
                <pre>{log_text}</pre>
            </div>
        </div>
    }
}
