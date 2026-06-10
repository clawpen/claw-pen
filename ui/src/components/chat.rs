use crate::types::AgentContainer;
use gloo_net::websocket::futures::WebSocket;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use yew::events::MouseEvent;
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
    pub agents: Vec<AgentContainer>,
    pub on_close: Callback<()>,
    pub on_switch: Callback<AgentContainer>,
}

#[function_component(ChatPanel)]
pub fn chat_panel(props: &ChatPanelProps) -> Html {
    let messages = use_state(|| VecDeque::<ChatMessage>::new());
    let input_text = use_state(String::new);
    let is_connected = use_state(|| false);

    // Connect to agent WebSocket
    {
        let is_connected = is_connected.clone();
        let agent_id = props.agent.id.clone();

        use_effect_with(agent_id.clone(), move |_| {
            let window = web_sys::window().unwrap();
            let protocol = window.location().protocol().unwrap_or_else(|_| "ws:".to_string());
            let host = window.location().host().unwrap_or_else(|_| "localhost:3001".to_string());
            let ws_protocol = if protocol.starts_with("https") { "wss" } else { "ws" };
            let ws_url = format!("{}://{}/api/agents/{}/chat", ws_protocol, host, agent_id);

            match WebSocket::open(&ws_url) {
                Ok(_ws) => {
                    is_connected.set(true);
                }
                Err(e) => {
                    web_sys::console::log_1(&format!("WebSocket error: {:?}", e).into());
                    is_connected.set(false);
                }
            }

            || {}
        });
    }

    let on_input = {
        let input_text = input_text.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            input_text.set(input.value());
        })
    };

    let on_send = {
        let input_text = input_text.clone();
        let messages = messages.clone();

        Callback::from(move |()| {
            let text = (*input_text).clone();
            if text.is_empty() {
                return;
            }

            let user_msg = ChatMessage {
                role: "user".to_string(),
                content: text.clone(),
                timestamp: js_sys::Date::now() as i64,
            };

            let response_msg = ChatMessage {
                role: "assistant".to_string(),
                content: format!("Echo: {}", text),
                timestamp: js_sys::Date::now() as i64,
            };

            let mut msgs = (*messages).clone();
            if msgs.len() >= MAX_MESSAGES {
                msgs.pop_front();
            }
            msgs.push_back(user_msg);
            if msgs.len() >= MAX_MESSAGES {
                msgs.pop_front();
            }
            msgs.push_back(response_msg);
            messages.set(msgs);

            input_text.set(String::new());
        })
    };

    let on_send_click = {
        let on_send = on_send.clone();
        Callback::from(move |_e: MouseEvent| {
            on_send.emit(());
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

    let on_switch = {
        let on_switch = props.on_switch.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let agent_id = select.value();
            if let Some(agent) = props.agents.iter().find(|a| a.id == agent_id) {
                on_switch.emit(agent.clone());
            }
        })
    };

    let on_close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_e: MouseEvent| {
            on_close.emit(());
        })
    };

    html! {
        <div class="chat-panel">
            <div class="chat-header">
                <div class="chat-header-left">
                    <h3>{format!("Chat with {}", props.agent.name)}</h3>
                    <select class="agent-switcher" onchange={on_switch} value={props.agent.id.clone()}>
                        {for props.agents.iter().map(|agent| {
                            let health_emoji = match agent.health_status {
                                Some(ref hs) if hs.healthy => "🟢",
                                Some(_) => "🔴",
                                None => "⚪",
                            };
                            let can_chat = agent.status == crate::types::AgentStatus::Running && 
                                matches!(agent.health_status, Some(ref hs) if hs.healthy);
                            html! {
                                <option value={agent.id.clone()} disabled={!can_chat}>
                                    {format!("{} {} ({})", health_emoji, agent.name, agent.status)}
                                </option>
                            }
                        })}
                    </select>
                </div>
                <span class={if *is_connected { "status connected" } else { "status disconnected" }}>
                    {if *is_connected { "Connected" } else { "Disconnected" }}
                </span>
                <button class="btn-close" onclick={on_close_click}>{"×"}</button>
            </div>

            <div class="chat-messages">
                if messages.is_empty() {
                    <div class="empty-chat">
                        <p>{"Start a conversation with this agent..."}</p>
                    </div>
                } else {
                    {for messages.iter().map(|msg| {
                        let is_user = msg.role == "user";
                        html! {
                            <div class={if is_user { "message user" } else { "message assistant" }}>
                                <div class="message-content">{&msg.content}</div>
                            </div>
                        }
                    })}
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
                    onclick={on_send_click}
                    disabled={!*is_connected || (*input_text).is_empty()}
                >
                    {"Send"}
                </button>
            </div>
        </div>
    }
}
