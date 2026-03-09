use crate::api;
use crate::components::tabbed_chat::TabbedChat;
use crate::components::settings::SettingsModal;
use crate::types::{AgentContainer, AgentStatus};
use yew::events::MouseEvent;
use yew::prelude::*;

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    // TODO: Fetch agents from API
    let agents = use_state(Vec::new);
    let open_chats = use_state(Vec::<AgentContainer>::new);
    let show_settings = use_state(|| false);

    let agents_clone = agents.clone();
    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(fetched) = api::fetch_agents().await {
                agents_clone.set(fetched);
            }
        });
        || ()
    });

    let on_close_tab = {
        let open_chats = open_chats.clone();
        Callback::from(move |agent_id: String| {
            let mut chats = (*open_chats).clone();
            chats.retain(|a| a.id != agent_id);
            open_chats.set(chats);
        })
    };

    let on_open_settings = {
        let show_settings = show_settings.clone();
        Callback::from(move |_| {
            show_settings.set(true);
        })
    };

    let on_close_settings = {
        let show_settings = show_settings.clone();
        Callback::from(move |()| {
            show_settings.set(false);
        })
    };

    html! {
        <div class="dashboard">
            <div class="toolbar">
                <button class="btn-primary">{"+ New Agent"}</button>
                <button class="btn-settings" onclick={on_open_settings}>{"⚙️"}</button>
            </div>

            <div class="agents-grid">
                if agents.is_empty() {
                    <div class="empty-state">
                        <p>{"No agents yet. Create one to get started!"}</p>
                    </div>
                } else {
                    {for agents.iter().map(|agent| {
                        let open_chat = {
                            let open_chats = open_chats.clone();
                            let agent = agent.clone();
                            Callback::from(move |()| {
                                let mut chats = (*open_chats).clone();
                                // Only add if not already open
                                if !chats.iter().any(|a| a.id == agent.id) {
                                    chats.push(agent.clone());
                                    open_chats.set(chats);
                                }
                            })
                        };
                        html! { <AgentCard agent={agent.clone()} on_chat={open_chat} /> }
                    })}
                }
            </div>

            if !(*open_chats).is_empty() {
                <TabbedChat open_agents={(*open_chats).clone()} on_close_tab={on_close_tab} />
            }

            if *show_settings {
                <SettingsModal on_close={on_close_settings} />
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AgentCardProps {
    pub agent: AgentContainer,
    pub on_chat: Callback<()>,
}

#[function_component(AgentCard)]
fn agent_card(props: &AgentCardProps) -> Html {
    let status_class = match props.agent.status {
        AgentStatus::Running => "status-running",
        AgentStatus::Stopped => "status-stopped",
        AgentStatus::Starting => "status-starting",
        AgentStatus::Stopping => "status-stopping",
        AgentStatus::Error => "status-error",
    };

    let status_text = match props.agent.status {
        AgentStatus::Running => "Running",
        AgentStatus::Stopped => "Stopped",
        AgentStatus::Starting => "Starting...",
        AgentStatus::Stopping => "Stopping...",
        AgentStatus::Error => "Error",
    };

    let can_chat = props.agent.status == AgentStatus::Running;
    let on_chat_click = {
        let on_chat = props.on_chat.clone();
        Callback::from(move |_e: MouseEvent| {
            on_chat.emit(());
        })
    };

    html! {
        <div class="agent-card">
            <div class="agent-header">
                <h3>{&props.agent.name}</h3>
                <span class={status_class}>{status_text}</span>
            </div>
            <div class="agent-body">
                <div class="info-row">
                    <span class="label">{"Provider:"}</span>
                    <span class={if props.agent.config.llm_provider.is_local() { "value local" } else { "value" }}>
                        {props.agent.config.llm_provider.display_name()}
                    </span>
                </div>
                <div class="info-row">
                    <span class="label">{"Memory:"}</span>
                    <span class="value">{format!("{} MB", props.agent.config.memory_mb)}</span>
                </div>
                <div class="info-row">
                    <span class="label">{"CPU:"}</span>
                    <span class="value">{format!("{} cores", props.agent.config.cpu_cores)}</span>
                </div>
                if let Some(ref ip) = props.agent.tailscale_ip {
                    <div class="info-row">
                        <span class="label">{"Tailscale:"}</span>
                        <span class="value">{ip}</span>
                    </div>
                }
            </div>
            <div class="agent-actions">
                if can_chat {
                    <button class="btn-chat" onclick={on_chat_click}>{"Chat"}</button>
                }
                if props.agent.status == AgentStatus::Running {
                    <button class="btn-stop">{"Stop"}</button>
                } else if props.agent.status == AgentStatus::Stopped {
                    <button class="btn-start">{"Start"}</button>
                }
                <button class="btn-config">{"Config"}</button>
            </div>
        </div>
    }
}
