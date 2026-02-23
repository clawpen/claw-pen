use crate::types::{AgentContainer, AgentStatus};
use crate::api;
use yew::prelude::*;
use yew::suspension::{Suspension, SuspensionResult};

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    // TODO: Fetch agents from API
    let agents = use_state(Vec::new);
    
    let agents_clone = agents.clone();
    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(fetched) = api::fetch_agents().await {
                agents_clone.set(fetched);
            }
        });
        || ()
    });

    html! {
        <div class="dashboard">
            <div class="toolbar">
                <button class="btn-primary">{"+ New Agent"}</button>
            </div>
            
            <div class="agents-grid">
                if agents.is_empty() {
                    <div class="empty-state">
                        <p>{"No agents yet. Create one to get started!"}</p>
                    </div>
                } else {
                    for agents.iter().map(|agent| {
                        html! { <AgentCard agent={agent.clone()} /> }
                    })
                }
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AgentCardProps {
    pub agent: AgentContainer,
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
