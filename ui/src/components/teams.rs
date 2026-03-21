use crate::api::{fetch_agents, fetch_teams, fetch_team_roles, assign_team_role, remove_team_role};
use crate::types::{Team, TeamRoleAssignment, AgentContainer, AgentStatus};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use std::collections::HashMap;

#[derive(Properties, PartialEq)]
pub struct TeamsProps {
    pub auth_token: String,
}

#[function_component(Teams)]
pub fn teams_component(_props: &TeamsProps) -> Html {
    let teams = use_state(|| Vec::<Team>::new());
    let agents = use_state(|| Vec::<AgentContainer>::new());
    let selected_team_id = use_state(|| Option::<String>::None);
    let roles = use_state(|| HashMap::<String, TeamRoleAssignment>::new());
    let selected_agents = use_state(|| HashMap::<String, String>::new()); // Track selected agents for each role
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let success_message = use_state(|| None::<String>);

    // Load teams on mount
    {
        let teams = teams.clone();
        let loading = loading.clone();
        let error = error.clone();

        use_effect_with((), move |_| {
            let teams = teams.clone();
            spawn_local(async move {
                loading.set(true);
                match fetch_teams().await {
                    Ok(t) => {
                        teams.set(t);
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load teams: {}", e)));
                        loading.set(false);
                    }
                }
            });
            || ()
        });
    }

    // Load agents on mount
    {
        let agents = agents.clone();
        use_effect_with((), move |_| {
            let agents = agents.clone();
            spawn_local(async move {
                match fetch_agents().await {
                    Ok(a) => agents.set(a),
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to load agents: {}", e).into());
                    }
                }
            });
            || ()
        });
    }

    // Load roles when team is selected
    {
        let selected_team_id = selected_team_id.clone();
        let roles = roles.clone();

        use_effect_with(selected_team_id.clone(), move |team_id| {
            if let Some(ref team_id) = **team_id {
                let team_id = team_id.clone();
                let roles = roles.clone();
                spawn_local(async move {
                    match fetch_team_roles(&team_id).await {
                        Ok(r) => {
                            let mut map = HashMap::new();
                            for assignment in r {
                                map.insert(assignment.intent.clone(), assignment);
                            }
                            roles.set(map);
                        }
                        Err(e) => {
                            web_sys::console::error_1(&format!("Failed to load roles: {}", e).into());
                        }
                    }
                });
            }
            || ()
        });
    }

    // Handle team selection
    let on_select_team = {
        let selected_team_id = selected_team_id.clone();
        Callback::from(move |team_id: String| {
            selected_team_id.set(Some(team_id));
        })
    };

    // Handle role assignment
    let on_assign_role = {
        let roles = roles.clone();
        let error = error.clone();
        let success = success_message.clone();

        Callback::from(move |(team_id, intent, agent_id): (String, String, String)| {
            let team_id = team_id.clone();
            let intent = intent.clone();
            let roles = roles.clone();
            let error = error.clone();
            let success = success.clone();

            spawn_local(async move {
                match assign_team_role(&team_id, &intent, &agent_id, "user").await {
                    Ok(assignment) => {
                        let mut updated = (*roles).clone();
                        updated.insert(intent.clone(), assignment);
                        roles.set(updated);
                        error.set(None);
                        success.set(Some(format!("Assigned {} to {} role", agent_id, intent.replace("_", " "))));

                        // Clear success message after 3 seconds
                        let success_clone = success.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            gloo_timers::callback::Timeout::new(3000, move || {
                                success_clone.set(None);
                            }).forget();
                        });
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to assign role: {}", e)));
                        success.set(None);
                    }
                }
            });
        })
    };

    // Handle role removal
    let on_remove_role = {
        let roles = roles.clone();
        let error = error.clone();
        let success = success_message.clone();

        Callback::from(move |(team_id, intent): (String, String)| {
            let team_id = team_id.clone();
            let intent = intent.clone();
            let roles = roles.clone();
            let error = error.clone();
            let success = success.clone();

            spawn_local(async move {
                match remove_team_role(&team_id, &intent).await {
                    Ok(_) => {
                        let mut updated = (*roles).clone();
                        updated.remove(&intent);
                        roles.set(updated);
                        error.set(None);
                        success.set(Some(format!("Removed {} role assignment", intent.replace("_", " "))));

                        // Clear success message after 3 seconds
                        let success_clone = success.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            gloo_timers::callback::Timeout::new(3000, move || {
                                success_clone.set(None);
                            }).forget();
                        });
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to remove role: {}", e)));
                        success.set(None);
                    }
                }
            });
        })
    };

    if *loading {
        return html! {
            <div class="p-6">
                <div class="flex items-center justify-center h-64">
                    <p class="text-gray-600">{"Loading teams..."}</p>
                </div>
            </div>
        };
    }

    // Find selected team outside of html! macro
    let selected_team = if let Some(ref team_id) = *selected_team_id {
        let teams_vec = (*teams).clone();
        teams_vec.iter().find(|t| &t.id == team_id).cloned()
    } else {
        None
    };

    html! {
        <div class="teams-container">
            // Header
            <div class="mb-6">
                <h2 class="text-2xl font-bold mb-2">{"Teams & Role Assignments"}</h2>
                <p class="text-gray-600">{"Manage your teams and assign agents to specialist roles"}</p>
            </div>

            // Success/Error Messages
            if let Some(ref success) = *success_message {
                <div class="mb-4 p-3 bg-green-100 border border-green-400 text-green-700 rounded flex items-center justify-between">
                    <span>{success}</span>
                    <button
                        class="text-green-700 hover:text-green-900"
                        onclick={Callback::from(move |_| success_message.set(None))}
                    >
                        {"✕"}
                    </button>
                </div>
            }

            if let Some(ref err) = *error {
                <div class="mb-4 p-3 bg-red-100 border border-red-400 text-red-700 rounded flex items-center justify-between">
                    <span>{err}</span>
                    <button
                        class="text-red-700 hover:text-red-900"
                        onclick={Callback::from(move |_| error.set(None))}
                    >
                        {"✕"}
                    </button>
                </div>
            }

            // Team Selector
            <div class="mb-6">
                <h3 class="text-lg font-semibold mb-3">{"Select Team"}</h3>
                if teams.is_empty() {
                    <p class="text-gray-500 italic">{"No teams available"}</p>
                } else {
                    <div class="flex flex-wrap gap-2">
                        {for teams.iter().map(|team| {
                            let team_id = team.id.clone();
                            let is_selected = selected_team_id.as_ref() == Some(&team_id);
                            let onclick = on_select_team.reform({
                                let team_id = team_id.clone();
                                move |_| team_id.clone()
                            });

                            html! {
                                <button
                                    key={team_id.clone()}
                                    class={format!("px-4 py-2 rounded-lg font-medium transition {}",
                                        if is_selected {
                                            "bg-blue-500 text-white hover:bg-blue-600"
                                        } else {
                                            "bg-gray-200 text-gray-700 hover:bg-gray-300"
                                        }
                                    )}
                                    onclick={onclick}
                                >
                                    {team.name.clone()}
                                </button>
                            }
                        })}
                    </div>
                }
            </div>

            // Team Roles
            {if let Some(ref team) = selected_team {
                let team_id = team.id.clone();
                html! {
                    <div class="team-roles">
                        <h3 class="text-lg font-semibold mb-4">{team.name.clone()}</h3>
                        {if let Some(ref desc) = team.description {
                            html! {
                                <p class="text-gray-600 mb-4">{desc.clone()}</p>
                            }
                        } else {
                            html! { <p class="text-gray-600 mb-4">{"No description"}</p> }
                        }}

                        // Roles Grid
                        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                            {for team.agents.iter().map(|(intent, team_agent)| {
                                let assigned_agent = roles.get(intent);
                                let intent_clone = intent.clone();
                                let team_id_clone = team_id.clone();
                                let description = team_agent.description.clone();
                                let default_agent = team_agent.agent.clone();
                                let selected_agents = selected_agents.clone();

                                html! {
                                    <div key={intent.clone()} class="border rounded-lg p-4 bg-white hover:shadow-md transition">
                                        <div class="mb-3">
                                            <h4 class="font-semibold text-lg capitalize text-blue-600">
                                                {intent.clone().replace("_", " ")}
                                            </h4>
                                            <p class="text-sm text-gray-600 mt-1">{description}</p>
                                        </div>

                                        // Current Assignment
                                        <div class="mb-3">
                                            {if let Some(ref assignment) = assigned_agent {
                                                html! {
                                                    <div class="p-2 bg-green-50 border border-green-200 rounded">
                                                        <p class="text-xs text-green-700 font-medium">{"Currently Assigned:"}</p>
                                                        <p class="text-sm font-bold text-green-800">{assignment.agent_id.clone()}</p>
                                                        <p class="text-xs text-green-600 mt-1">
                                                            {"Since "}{assignment.assigned_at.split('T').next().unwrap_or("").split('.').next().unwrap_or("")}
                                                        </p>
                                                    </div>
                                                }
                                            } else {
                                                html! {
                                                    <div class="p-2 bg-gray-50 border border-gray-200 rounded">
                                                        <p class="text-xs text-gray-600">{"Default Agent:"}</p>
                                                        <p class="text-sm font-mono text-gray-800">{default_agent}</p>
                                                    </div>
                                                }
                                            }}
                                        </div>

                                        // Agent Selection
                                        <div class="space-y-2">
                                            <select
                                                class="w-full border rounded px-3 py-2 text-sm"
                                                onchange={Callback::from({
                                                    let intent_clone = intent.clone();
                                                    let selected_agents = selected_agents.clone();
                                                    move |e: Event| {
                                                        let target = e.target().unwrap();
                                                        let select: web_sys::HtmlSelectElement = target.unchecked_into();
                                                        let value = select.value();
                                                        let mut updated = (*selected_agents).clone();
                                                        if value.is_empty() {
                                                            updated.remove(&intent_clone);
                                                        } else {
                                                            updated.insert(intent_clone.clone(), value);
                                                        }
                                                        selected_agents.set(updated);
                                                    }
                                                })}
                                            >
                                                <option value="">{"Select agent to assign..."}</option>
                                                {for agents.iter().filter(|a| a.status == AgentStatus::Running).map(|agent| {
                                                    let agent_id = agent.id.clone();
                                                    let is_assigned = assigned_agent.as_ref().map(|a| &a.agent_id == &agent_id).unwrap_or(false);
                                                    html! {
                                                        <option
                                                            value={agent_id.clone()}
                                                            selected={is_assigned}
                                                        >
                                                            {format!("{} ({})", agent.name, agent.id)}
                                                        </option>
                                                    }
                                                })}
                                            </select>

                                            <div class="flex gap-2">
                                                {if assigned_agent.is_some() {
                                                    html! {
                                                        <>
                                                            <button
                                                                class="flex-1 bg-blue-500 hover:bg-blue-600 text-white px-3 py-2 rounded text-sm font-medium transition"
                                                                onclick={on_assign_role.reform({
                                                                    let team_id_clone = team_id_clone.clone();
                                                                    let intent_clone = intent.clone();
                                                                    let selected_agents = selected_agents.clone();
                                                                    move |_| {
                                                                        let agent_id = selected_agents.get(&intent_clone)
                                                                            .cloned()
                                                                            .unwrap_or_else(|| String::new());
                                                                        (team_id_clone.clone(), intent_clone.clone(), agent_id)
                                                                    }
                                                                })}
                                                            >
                                                                {"Change"}
                                                            </button>
                                                            <button
                                                                class="flex-1 bg-red-500 hover:bg-red-600 text-white px-3 py-2 rounded text-sm font-medium transition"
                                                                onclick={on_remove_role.reform({
                                                                    let team_id_clone = team_id_clone.clone();
                                                                    let intent_clone = intent.clone();
                                                                    move |_| (team_id_clone.clone(), intent_clone.clone())
                                                                })}
                                                            >
                                                                {"Remove"}
                                                            </button>
                                                        </>
                                                    }
                                                } else {
                                                    let has_selection = selected_agents.contains_key(&intent_clone);
                                                    html! {
                                                        <button
                                                            class={format!("w-full bg-blue-500 hover:bg-blue-600 text-white px-3 py-2 rounded text-sm font-medium transition {}", if has_selection { "" } else { "opacity-50" })}
                                                            onclick={on_assign_role.reform({
                                                                let team_id_clone = team_id_clone.clone();
                                                                let intent_clone = intent.clone();
                                                                let selected_agents = selected_agents.clone();
                                                                move |_| {
                                                                    let agent_id = selected_agents.get(&intent_clone)
                                                                        .cloned()
                                                                        .unwrap_or_else(|| String::new());
                                                                    (team_id_clone.clone(), intent_clone.clone(), agent_id)
                                                                }
                                                            })}
                                                            disabled={!has_selection}
                                                            title={if has_selection { "Assign this agent" } else { "Select an agent first" }}
                                                        >
                                                            {"Assign"}
                                                        </button>
                                                    }
                                                }}
                                            </div>
                                        </div>
                                    </div>
                                }
                            })}
                        </div>

                        // Helper Text
                        <div class="mt-6 p-4 bg-blue-50 border border-blue-200 rounded">
                            <h4 class="font-semibold text-blue-900 mb-2">{"💡 How it works"}</h4>
                            <ul class="text-sm text-blue-800 space-y-1">
                                <li>{"• Each team has specialist roles (e.g., Time Analyst, Designer)"}</li>
                                <li>{"• Assign any running agent to a role dynamically"}</li>
                                <li>{"• Agents can have multiple roles across teams"}</li>
                                <li>{"• Remove assignments to fall back to default agents"}</li>
                                <li>{"• Changes take effect immediately"}</li>
                            </ul>
                        </div>
                    </div>
                }
            } else {
                // No team selected
                html! {
                    <div class="text-center py-16">
                        <div class="text-6xl mb-4">{"👆"}</div>
                        <h3 class="text-xl font-semibold text-gray-700 mb-2">{"Select a Team"}</h3>
                        <p class="text-gray-500">{"Choose a team from the buttons above to manage its role assignments"}</p>
                    </div>
                }
            }}

            // Available Agents Reference
            <div class="mt-8 border-t pt-6">
                <h3 class="text-lg font-semibold mb-3">{"Available Agents"}</h3>
                if agents.is_empty() {
                    <p class="text-gray-500 italic">{"No running agents"}</p>
                } else {
                    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                        {for agents.iter().map(|agent| {
                            let is_running = agent.status == AgentStatus::Running;
                            html! {
                                <div key={agent.id.clone()} class={format!("border rounded p-3 text-sm {}",
                                    if is_running { "bg-green-50 border-green-200" } else { "bg-gray-50 border-gray-200" }
                                )}>
                                    <div class="flex items-center gap-2 mb-1">
                                        <span class={format!("w-2 h-2 rounded-full {}",
                                            if is_running { "bg-green-500" } else { "bg-gray-400" }
                                        )}></span>
                                        <span class="font-medium">{agent.name.clone()}</span>
                                    </div>
                                    <p class="text-gray-600 text-xs">{agent.id.clone()}</p>
                                </div>
                            }
                        })}
                    </div>
                }
            </div>
        </div>
    }
}
