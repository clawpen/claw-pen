use crate::tauri;
use crate::types::AgentContainer;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use yew::events::MouseEvent;
use yew::prelude::*;

const MAX_MESSAGES: usize = 100;

static LISTENER_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: i64,
}

#[derive(Properties, PartialEq)]
pub struct ChatPanelProps {
    pub agent: AgentContainer,
    pub on_close: Callback<()>,
}

pub enum ChatMsg {
    Connected(bool),
    Authenticated(bool),
    MessageReceived(ChatMessage),
    Error(String),
    InputChanged(String),
    ClearInput,
    ListenersReady,
}

pub struct ChatPanel {
    messages: VecDeque<ChatMessage>,
    input_text: String,
    is_connected: bool,
    is_authenticated: bool,
    error: Option<String>,
    listeners_ready: bool,
}

impl Component for ChatPanel {
    type Message = ChatMsg;
    type Properties = ChatPanelProps;

    fn create(ctx: &Context<Self>) -> Self {
        let panel = Self {
            messages: VecDeque::new(),
            input_text: String::new(),
            is_connected: false,
            is_authenticated: false,
            error: None,
            listeners_ready: false,
        };

        // Set up listeners and connect
        Self::setup_listeners(ctx);
        Self::connect(ctx);

        panel
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ChatMsg::Connected(connected) => {
                self.is_connected = connected;
                if !connected {
                    self.is_authenticated = false;
                }
                true
            }
            ChatMsg::Authenticated(auth) => {
                self.is_authenticated = auth;
                if auth {
                    self.error = None;
                }
                true
            }
            ChatMsg::MessageReceived(message) => {
                if self.messages.len() >= MAX_MESSAGES {
                    self.messages.pop_front();
                }
                self.messages.push_back(message);
                true
            }
            ChatMsg::Error(err) => {
                self.error = Some(err);
                true
            }
            ChatMsg::InputChanged(text) => {
                self.input_text = text;
                true
            }
            ChatMsg::ClearInput => {
                self.input_text.clear();
                true
            }
            ChatMsg::ListenersReady => {
                self.listeners_ready = true;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let input_text = self.input_text.clone();
        let on_send = link.callback(move |_| {
            if !input_text.is_empty() {
                let text = input_text.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = tauri::send_chat_message(&text).await {
                        web_sys::console::log_1(&format!("Send error: {}", e).into());
                    }
                });
            }
            ChatMsg::ClearInput
        });

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

        let on_close_click = {
            let on_close = ctx.props().on_close.clone();
            Callback::from(move |_e: MouseEvent| {
                on_close.emit(());
            })
        };

        let status_text = if !self.is_connected {
            "Connecting..."
        } else if !self.is_authenticated {
            "Authenticating..."
        } else {
            "Connected"
        };

        html! {
            <div class="chat-panel">
                <div class="chat-header">
                    <h3>{format!("Chat with {}", ctx.props().agent.name)}</h3>
                    <span class={if self.is_authenticated { "status connected" } else { "status disconnected" }}>
                        {status_text}
                    </span>
                    <button class="btn-close" onclick={on_close_click}>{"×"}</button>
                </div>

                if let Some(ref err) = self.error {
                    <div class="chat-error">
                        {err}
                    </div>
                }

                <div class="chat-messages">
                    if self.messages.is_empty() {
                        <div class="empty-chat">
                            <p>{"Start a conversation with this agent..."}</p>
                        </div>
                    } else {
                        {for self.messages.iter().map(|msg| {
                            let is_user = msg.role == "user";
                            html! {
                                <div class={if is_user { "message user" } else { "message assistant" }}>
                                    <div class="message-role">{if is_user { "You" } else { &ctx.props().agent.name }}</div>
                                    <div class="message-content">{&msg.content}</div>
                                </div>
                            }
                        })}
                    }
                </div>

                <div class="chat-input">
                    <textarea
                        placeholder="Type a message..."
                        value={self.input_text.clone()}
                        oninput={link.callback(|e: InputEvent| {
                            let input: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
                            ChatMsg::InputChanged(input.value())
                        })}
                        onkeypress={on_keypress}
                        disabled={!self.is_authenticated}
                    />
                    <button
                        class="btn-send"
                        onclick={on_send_click}
                        disabled={!self.is_authenticated || self.input_text.is_empty()}
                    >
                        {"Send"}
                    </button>
                </div>
            </div>
        }
    }
}

impl ChatPanel {
    fn setup_listeners(ctx: &Context<Self>) {
        let link = ctx.link().clone();
        
        wasm_bindgen_futures::spawn_local(async move {
            // Listen to ws-connected
            let link_clone = link.clone();
            if let Ok(_unlisten) = tauri::listen_event("ws-connected", move |value| {
                if let Some(connected) = value.as_bool() {
                    link_clone.send_message(ChatMsg::Connected(connected));
                }
            }).await {
                LISTENER_COUNT.fetch_add(1, Ordering::SeqCst);
            }

            // Listen to ws-authenticated
            let link_clone = link.clone();
            if let Ok(_unlisten) = tauri::listen_event("ws-authenticated", move |value| {
                if let Some(auth) = value.as_bool() {
                    link_clone.send_message(ChatMsg::Authenticated(auth));
                }
            }).await {
                LISTENER_COUNT.fetch_add(1, Ordering::SeqCst);
            }

            // Listen to ws-message
            let link_clone = link.clone();
            if let Ok(_unlisten) = tauri::listen_event("ws-message", move |value| {
                // Parse the message
                if let Some(obj) = value.as_object() {
                    // Check for chat events
                    if let Some(event_type) = obj.get("event").and_then(|v| v.as_str()) {
                        if event_type == "chat.message" || event_type == "chat.response" {
                            if let Some(params) = obj.get("params") {
                                if let (Some(role), Some(content)) = (
                                    params.get("role").and_then(|v| v.as_str()),
                                    params.get("content").and_then(|v| v.as_str())
                                ) {
                                    link_clone.send_message(ChatMsg::MessageReceived(ChatMessage {
                                        role: role.to_string(),
                                        content: content.to_string(),
                                        timestamp: js_sys::Date::now() as i64,
                                    }));
                                    return;
                                }
                            }
                        }
                    }
                    
                    // Check for result with message
                    if let Some(result) = obj.get("result") {
                        if let (Some(role), Some(content)) = (
                            result.get("role").and_then(|v| v.as_str()),
                            result.get("content").and_then(|v| v.as_str())
                        ) {
                            link_clone.send_message(ChatMsg::MessageReceived(ChatMessage {
                                role: role.to_string(),
                                content: content.to_string(),
                                timestamp: js_sys::Date::now() as i64,
                            }));
                        }
                    }
                }
            }).await {
                LISTENER_COUNT.fetch_add(1, Ordering::SeqCst);
            }

            // Listen to ws-error
            let link_clone = link.clone();
            if let Ok(_unlisten) = tauri::listen_event("ws-error", move |value| {
                if let Some(err) = value.as_str() {
                    link_clone.send_message(ChatMsg::Error(err.to_string()));
                } else if let Some(obj) = value.as_object() {
                    if let Some(msg) = obj.get("message").and_then(|v| v.as_str()) {
                        link_clone.send_message(ChatMsg::Error(msg.to_string()));
                    } else {
                        link_clone.send_message(ChatMsg::Error("WebSocket error".to_string()));
                    }
                } else {
                    link_clone.send_message(ChatMsg::Error("WebSocket error".to_string()));
                }
            }).await {
                LISTENER_COUNT.fetch_add(1, Ordering::SeqCst);
            }

            link.send_message(ChatMsg::ListenersReady);
        });
    }

    fn connect(ctx: &Context<Self>) {
        let port = ctx.props().agent.config.env_vars
            .get("PORT")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(18790);
        
        let url = format!("ws://localhost:{}/ws", port);
        
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = tauri::connect_websocket(&url).await {
                web_sys::console::log_1(&format!("Connection error: {}", e).into());
            }
        });
    }
}
