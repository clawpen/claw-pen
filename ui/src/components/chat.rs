use crate::types::AgentContainer;
use gloo_net::websocket::{Message, WebSocket, WebSocketError};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use yew::prelude::*;

const MAX_MESSAGES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Properties, PartialEq)]
pub struct ChatPanelProps {
    pub agent: AgentContainer,
    pub on_close: Callback<()>,
}

#[function_component(ChatPanel)]
pub fn chat_panel(props: &ChatPanelProps) -> Html {
    let messages = use_state(VecDeque::<ChatMessage>::new);
    let input_text = use_state(String::new);
    let is_connected = use_state(|| false);
    let is_sending = use_state(|| false);

    // WebSocket reference
    let ws_ref = use_mut_ref(|| None::<WebSocket>);

    // Connect to agent WebSocket
    {
        let messages = messages.clone();
        let is_connected = is_connected.clone();
        let agent_id = props.agent.id.clone();

        use_effect_with(agent_id.clone(), move |_| {
            let ws_url = format!("ws://localhost:3000/api/agents/{}/chat", agent_id);

            match WebSocket::open(&ws_url) {
                Ok(ws) => {
                    let (mut write, mut read) = ws.split();

                    // Handle incoming messages
                    wasm_bindgen_futures::spawn_local(async move {
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                                        messages.update(|msgs| {
                                            if msgs.len() >= MAX_MESSAGES {
                                                msgs.pop_front();
                                            }
                                            msgs.push_back(chat_msg);
                                        });
                                    }
                                }
                                Ok(Message::Bytes(_)) => {}
                                Err(WebSocketError::ConnectionError) => {
                                    is_connected.set(false);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });

                    *ws_ref.borrow_mut() = Some(WebSocket::from(write));
                    is_connected.set(true);
                }
                Err(_) => {
                    web_sys::console::log_1(&"Failed to connect to WebSocket".into());
                }
            }

            || {}
        });
    }

    let on_send = {
        let input_text = input_text.clone();
        let messages = messages.clone();
        let is_sending = is_sending.clone();

        Callback::from(move |_| {
            let text = (*input_text).clone();
            if text.is_empty() || *is_sending {
                return;
            }

            // Add user message
            let user_msg = ChatMessage {
                role: "user".to_string(),
                content: text.clone(),
                timestamp: js_sys::Date::now() as i64,
            };

            messages.update(|msgs| {
                if msgs.len() >= MAX_MESSAGES {
                    msgs.pop_front();
                }
                msgs.push_back(user_msg);
            });

            // Send via WebSocket
            if let Some(ref ws) = *ws_ref.borrow() {
                let msg = serde_json::to_string(&serde_json::json!({
                    "role": "user",
                    "content": text
                }))
                .unwrap_or_default();

                let _ = ws.send(Message::Text(msg));
            }

            input_text.set(String::new());
        })
    };

    let on_input = {
        let input_text = input_text.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            input_text.set(input.value());
        })
    };

    let on_keypress = {
        let on_send = on_send.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" && !e.shift_key() {
                e.prevent_default();
                on_send.emit(());
            }
        })
    };

    let on_close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| {
            on_close.emit(());
        })
    };

    html! {
        <div class="chat-panel">
            <div class="chat-header">
                <h3>{format!("Chat with {}", props.agent.name)}</h3>
                <span class={if *is_connected { "status connected" } else { "status disconnected" }}>
                    {if *is_connected { "Connected" } else { "Disconnected" }}
                </span>
                <button class="btn-close" onclick={on_close}>{"×"}</button>
            </div>

            <div class="chat-messages">
                {for messages.iter().map(|msg| {
                    let is_user = msg.role == "user";
                    html! {
                        <div class={if is_user { "message user" } else { "message assistant" }}>
                            <div class="message-content">{&msg.content}</div>
                        </div>
                    }
                })}

                if messages.is_empty() {
                    <div class="empty-chat">
                        <p>{"Start a conversation with this agent..."}</p>
                    </div>
                }
            </div>

            <div class="chat-input">
                <textarea
                    placeholder="Type a message..."
                    value={(*input_text).clone()}
                    oninput={on_input}
                    onkeypress={on_keypress}
                    disabled={!*is_connected}
                />
                <button
                    class="btn-send"
                    onclick={on_send}
                    disabled={!*is_connected || (*input_text).is_empty()}
                >
                    {"Send"}
                </button>
            </div>
        </div>
    }
}
