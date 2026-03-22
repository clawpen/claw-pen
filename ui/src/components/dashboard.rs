use crate::api::{fetch_agents, fetch_teams, fetch_team_roles, assign_team_role, remove_team_role};
use crate::components::chat::ChatPanel;
use crate::types::{AgentContainer, AgentStatus, Team, TeamRoleAssignment};
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlSelectElement;
use yew::events::MouseEvent;
use yew::prelude::*;

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    let agents = use_state(Vec::new);
    let teams = use_state(Vec::new);
    let team_roles = use_state(HashMap::<String, Vec<TeamRoleAssignment>>::new);
    let chat_agent = use_state(|| None::<AgentContainer>);
    let team_modal_agent = use_state(|| None::<AgentContainer>);
    let selected_team = use_state(|| None::<String>);
    let selected_role = use_state(|| None::<String>);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let success = use_state(|| None::<String>);

    // Load agents on mount
    {
        let agents = agents.clone();
        let loading = loading.clone();
        spawn_local(async move {
            match fetch_agents().await {
                Ok(fetched) => {
                    agents.set(fetched);
                    loading.set(false);
                }
                Err(e) => {
                    loading.set(false);
                    web_sys::console::error_1(&format!("Failed to load agents: {}", e).into());
                }
            }
        });
    }

    // Load teams on mount
    {
        let teams = teams.clone();
        spawn_local(async move {
            match fetch_teams().await {
                Ok(t) => teams.set(t),
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to load teams: {}", e).into());
                }
            }
        });
    }

    let on_close_chat = {
        let chat_agent = chat_agent.clone();
        Callback::from(move |()| {
            chat_agent.set(None);
        })
    };

    // Clone state handles before creating callbacks that move them
    let selected_team_for_close = selected_team.clone();
    let selected_role_for_close = selected_role.clone();
    let error_for_close = error.clone();
    let success_for_close = success.clone();
    let selected_team_for_team_change = selected_team.clone();
    let selected_role_for_team_change = selected_role.clone();
    let selected_role_for_role_change = selected_role.clone();
    let selected_team_for_assign = selected_team.clone();
    let selected_role_for_assign = selected_role.clone();
    let selected_team_for_remove = selected_team.clone();
    let selected_role_for_remove = selected_role.clone();

    // Open team assignment modal
    let on_open_teams = {
        let team_modal_agent = team_modal_agent.clone();
        let team_roles = team_roles.clone();
        let teams = teams.clone();
        let selected_team = selected_team.clone();
        let selected_role = selected_role.clone();

        Callback::from(move |agent: AgentContainer| {
            // Load roles for all teams
            let teams_clone = (*teams).clone();
            let team_roles = team_roles.clone();

            for team in &teams_clone {
                let team_id = team.id.clone();
                let team_roles = team_roles.clone();
                spawn_local(async move {
                    match fetch_team_roles(&team_id).await {
                        Ok(roles) => {
                            let mut all_roles = (*team_roles).clone();
                            all_roles.insert(team_id, roles);
                            team_roles.set(all_roles);
                        }
                        Err(e) => {
                            web_sys::console::error_1(&format!("Failed to load roles: {}", e).into());
                        }
                    }
                });
            }

            team_modal_agent.set(Some(agent));
            selected_team.set(None);
            selected_role.set(None);
        })
    };

    // Close team modal
    let on_close_team_modal = {
        let team_modal_agent = team_modal_agent.clone();
        Callback::from(move |()| {
            team_modal_agent.set(None);
            selected_team_for_close.set(None);
            selected_role_for_close.set(None);
            error_for_close.set(None);
            success_for_close.set(None);
        })
    };

    // Handle team selection
    let on_team_change = {
        Callback::from(move |team_id: String| {
            selected_team_for_team_change.set(Some(team_id));
            selected_role_for_team_change.set(None); // Reset role when team changes
        })
    };

    // Handle role selection
    let on_role_change = {
        Callback::from(move |role: String| {
            selected_role_for_role_change.set(Some(role));
        })
    };

    // Assign role to agent
    let on_assign_role = {
        let team_modal_agent = team_modal_agent.clone();
        let team_roles = team_roles.clone();
        let error = error.clone();
        let success = success.clone();
        let on_close_team_modal = on_close_team_modal.clone();

        Callback::from(move |()| {
            let agent = (*team_modal_agent).clone().unwrap();
            let team_id = (*selected_team_for_assign).clone().unwrap();
            let role = (*selected_role_for_assign).clone().unwrap();
            let team_roles = team_roles.clone();
            let error = error.clone();
            let success = success.clone();
            let on_close_team_modal = on_close_team_modal.clone();

            spawn_local(async move {
                match assign_team_role(&team_id, &role, &agent.id, "user").await {
                    Ok(_) => {
                        success.set(Some("Role assigned successfully!".to_string()));
                        // Reload team roles
                        match fetch_team_roles(&team_id).await {
                            Ok(roles) => {
                                let mut all_roles = (*team_roles).clone();
                                all_roles.insert(team_id.clone(), roles);
                                team_roles.set(all_roles);
                            }
                            Err(e) => {
                                web_sys::console::error_1(&format!("Failed to reload roles: {}", e).into());
                            }
                        }
                        // Close modal after short delay
                        let success_clone = success.clone();
                        spawn_local(async move {
                            gloo_timers::callback::Timeout::new(1500, move || {
                                success_clone.set(None);
                                on_close_team_modal.emit(());
                            }).forget();
                        });
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to assign role: {}", e)));
                    }
                }
            });
        })
    };

    // Remove role assignment
    let on_remove_role = {
        let team_roles = team_roles.clone();
        let error = error.clone();
        let success = success.clone();

        Callback::from(move |()| {
            let team_id = (*selected_team_for_remove).clone().unwrap();
            let role = (*selected_role_for_remove).clone().unwrap();
            let team_roles = team_roles.clone();
            let error = error.clone();
            let success = success.clone();

            spawn_local(async move {
                match remove_team_role(&team_id, &role).await {
                    Ok(_) => {
                        success.set(Some("Role removed successfully!".to_string()));
                        // Reload team roles
                        match fetch_team_roles(&team_id).await {
                            Ok(roles) => {
                                let mut all_roles = (*team_roles).clone();
                                all_roles.insert(team_id, roles);
                                team_roles.set(all_roles);
                            }
                            Err(e) => {
                                web_sys::console::error_1(&format!("Failed to reload roles: {}", e).into());
                            }
                        }
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to remove role: {}", e)));
                    }
                }
            });
        })
    };

    html! {
        <div class="dashboard-container">
            // Sidebar with agent list
            <div class="sidebar">
                <div class="sidebar-header">
                    <h1>{"🦞 Claw Pen"}</h1>
                </div>
                <div class="agent-list">
                    if *loading {
                        <div class="loading-state">{"Loading agents..."}</div>
                    } else if agents.is_empty() {
                        <div class="empty-state">{"No agents available"}</div>
                    } else {
                        {for agents.iter().map(|agent| {
                            let is_active = (*chat_agent).as_ref().map(|a| &a.id) == Some(&agent.id);
                            let open_chat = {
                                let chat_agent = chat_agent.clone();
                                let agent = agent.clone();
                                Callback::from(move |()| {
                                    chat_agent.set(Some(agent.clone()));
                                })
                            };
                            let on_open_teams = on_open_teams.clone();
                            let agent_clone = agent.clone();
                            let agent_for_card = agent.clone();
                            html! {
                                <AgentCard
                                    agent={agent_for_card}
                                    is_active={is_active}
                                    on_chat={open_chat}
                                    on_teams={on_open_teams.reform(move |_| agent_clone.clone())}
                                />
                            }
                        })}
                    }
                </div>
                <div class="sidebar-footer">
                    <button class="btn-refresh" onclick={
                        let agents = agents.clone();
                        Callback::from(move |_| {
                            let agents = agents.clone();
                            spawn_local(async move {
                                match fetch_agents().await {
                                    Ok(fetched) => agents.set(fetched),
                                    Err(e) => {
                                        web_sys::console::error_1(&format!("Failed to refresh: {}", e).into());
                                    }
                                }
                            });
                        })
                    }>{"↻ Refresh"}</button>
                </div>
            </div>

            // Main chat area
            <div class="main">
                if let Some(agent) = (*chat_agent).clone() {
                    <ChatPanel agent={agent} on_close={on_close_chat} />
                } else {
                    <div class="empty-main">
                        <div class="emoji">{"🦞"}</div>
                        <h2>{"Claw Pen"}</h2>
                        <p>{"Select an agent to start chatting"}</p>
                    </div>
                }
            </div>

            // Team assignment modal
            if let Some(agent) = (*team_modal_agent).clone() {
                <TeamAssignmentModal
                    agent={agent}
                    teams={(*teams).clone()}
                    team_roles={(*team_roles).clone()}
                    selected_team={(*selected_team).clone()}
                    selected_role={(*selected_role).clone()}
                    error={(*error).clone()}
                    success={(*success).clone()}
                    on_close={on_close_team_modal}
                    on_team_change={on_team_change}
                    on_role_change={on_role_change}
                    on_assign={on_assign_role}
                    on_remove={on_remove_role}
                />
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AgentCardProps {
    pub agent: AgentContainer,
    pub is_active: bool,
    pub on_chat: Callback<()>,
    pub on_teams: Callback<AgentContainer>,
}

#[function_component(AgentCard)]
fn agent_card(props: &AgentCardProps) -> Html {
    let status_class = match props.agent.status {
        AgentStatus::Running => "running",
        AgentStatus::Stopped => "stopped",
        AgentStatus::Starting => "starting",
        AgentStatus::Stopping => "stopping",
        AgentStatus::Error => "error",
    };

    let status_text = match props.agent.status {
        AgentStatus::Running => "running",
        AgentStatus::Stopped => "stopped",
        AgentStatus::Starting => "starting",
        AgentStatus::Stopping => "stopping",
        AgentStatus::Error => "error",
    };

    let can_chat = props.agent.status == AgentStatus::Running;
    let on_chat_click = {
        let on_chat = props.on_chat.clone();
        Callback::from(move |_e: MouseEvent| {
            on_chat.emit(());
        })
    };

    let on_teams_click = {
        let on_teams = props.on_teams.clone();
        let agent = props.agent.clone();
        Callback::from(move |_e: MouseEvent| {
            on_teams.emit(agent.clone());
        })
    };

    html! {
        <div class={format!("agent-card {}", if props.is_active { "active" } else { "" })}>
            <div class="agent-card-header">
                <div class="name">{&props.agent.name}</div>
                <div class={format!("status {}", status_class)}>{status_text}</div>
            </div>
            <div class="agent-card-actions">
                <button
                    class={format!("action-btn chat-btn {}", if !can_chat { "disabled" } else { "" })}
                    disabled={!can_chat}
                    onclick={on_chat_click}
                    title="Chat with agent"
                >{"💬"}</button>
                <button
                    class="action-btn teams-btn"
                    onclick={on_teams_click}
                    title="Assign to team"
                >{"👥"}</button>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct TeamAssignmentModalProps {
    pub agent: AgentContainer,
    pub teams: Vec<Team>,
    pub team_roles: HashMap<String, Vec<TeamRoleAssignment>>,
    pub selected_team: Option<String>,
    pub selected_role: Option<String>,
    pub error: Option<String>,
    pub success: Option<String>,
    pub on_close: Callback<()>,
    pub on_team_change: Callback<String>,
    pub on_role_change: Callback<String>,
    pub on_assign: Callback<()>,
    pub on_remove: Callback<()>,
}

#[function_component(TeamAssignmentModal)]
fn team_assignment_modal(props: &TeamAssignmentModalProps) -> Html {
    let on_close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_e: MouseEvent| {
            on_close.emit(());
        })
    };

    // Get available roles for selected team
    let available_roles = if let Some(ref team_id) = props.selected_team {
        props.teams.iter()
            .find(|t| &t.id == team_id)
            .map(|team| {
                team.agents.keys().cloned().collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Check if current role is assigned
    let is_assigned = if let (Some(ref team_id), Some(ref role)) = (&props.selected_team, &props.selected_role) {
        props.team_roles.get(team_id)
            .map(|roles| roles.iter().any(|r| &r.intent == role))
            .unwrap_or(false)
    } else {
        false
    };

    html! {
        <div class="modal-overlay" onclick={on_close_click.clone()}>
            <div class="modal-content" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="modal-header">
                    <h2>{format!("Assign {} to Team", props.agent.name)}</h2>
                    <button class="modal-close" onclick={on_close_click}>{"×"}</button>
                </div>

                <div class="modal-body">
                    if let Some(ref error) = props.error {
                        <div class="error-message">{error}</div>
                    }
                    if let Some(ref success) = props.success {
                        <div class="success-message">{success}</div>
                    }

                    <div class="form-group">
                        <label>{"Select Team:"}</label>
                        <select
                            onchange={
                                let on_team_change = props.on_team_change.clone();
                                Callback::from(move |e: yew::events::Event| {
                                    let value = e.target_unchecked_into::<HtmlSelectElement>().value();
                                    on_team_change.emit(value);
                                })
                            }
                        >
                            <option value="">{"-- Choose a team --"}</option>
                            {for props.teams.iter().map(|team| {
                                let selected = props.selected_team.as_ref().map(|s| s == &team.id).unwrap_or(false);
                                html! {
                                    <option value={team.id.clone()} selected={selected}>
                                        {&team.name}
                                    </option>
                                }
                            })}
                        </select>
                    </div>

                    if let Some(ref team_id) = props.selected_team {
                        <div class="form-group">
                            <label>{"Select Role:"}</label>
                            <select
                                onchange={
                                    let on_role_change = props.on_role_change.clone();
                                    Callback::from(move |e: yew::events::Event| {
                                        let value = e.target_unchecked_into::<HtmlSelectElement>().value();
                                        on_role_change.emit(value);
                                    })
                                }
                                disabled={available_roles.is_empty()}
                            >
                                <option value="">{"-- Choose a role --"}</option>
                                {for available_roles.iter().map(|role| {
                                    let selected = props.selected_role.as_ref().map(|s| s == role).unwrap_or(false);
                                    html! {
                                        <option value={role.clone()} selected={selected}>
                                            {role.replace("_", " ")}
                                        </option>
                                    }
                                })}
                            </select>
                        </div>

                        if !available_roles.is_empty() {
                            if props.selected_role.is_some() {
                                <div class="modal-actions">
                                    if !is_assigned {
                                        <button
                                            class="btn-primary"
                                            onclick={
                                                let on_assign = props.on_assign.clone();
                                                Callback::from(move |_e: MouseEvent| {
                                                    on_assign.emit(());
                                                })
                                            }
                                        >{"Assign Role"}</button>
                                    } else {
                                        <button
                                            class="btn-danger"
                                            onclick={
                                                let on_remove = props.on_remove.clone();
                                                Callback::from(move |_e: MouseEvent| {
                                                    on_remove.emit(());
                                                })
                                            }
                                        >{"Remove Assignment"}</button>
                                    }
                                </div>
                            }
                        } else {
                            <div class="info-message">{"No roles available for this team"}</div>
                        }

                        // Show current assignments for this team
                        if let Some(roles) = props.team_roles.get(team_id) {
                            if !roles.is_empty() {
                                <div class="current-assignments">
                                    <h4>{"Current Assignments:"}</h4>
                                    <ul>
                                        {for roles.iter().map(|assignment| {
                                            let is_this_agent = assignment.agent_id == props.agent.id;
                                            html! {
                                                <li class={if is_this_agent { "current-agent" } else { "" }}>
                                                    {format!("{}: {}", assignment.intent.replace("_", " "), assignment.agent_id)}
                                                    {if is_this_agent { " (This agent)" } else { "" }}
                                                </li>
                                            }
                                        })}
                                    </ul>
                                </div>
                            }
                        }
                    }
                </div>
            </div>
        </div>
    }
}
