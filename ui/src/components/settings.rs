use crate::api;
use yew::prelude::*;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamSettings {
    pub mode: TeamMode,
    pub enabled_roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TeamMode {
    #[serde(rename = "multiplexed")]
    Multiplexed,
    #[serde(rename = "isolated")]
    Isolated,
}

impl Default for TeamSettings {
    fn default() -> Self {
        Self {
            mode: TeamMode::Multiplexed,
            enabled_roles: vec![
                "pm".to_string(),
                "developer".to_string(),
                "qa".to_string(),
                "designer".to_string(),
                "devops".to_string(),
                "security".to_string(),
                "architect".to_string(),
            ],
        }
    }
}

impl TeamMode {
    fn display_name(&self) -> &str {
        match self {
            TeamMode::Multiplexed => "Multiplexed (1 container)",
            TeamMode::Isolated => "Isolated (7 containers)",
        }
    }
    
    fn description(&self) -> &str {
        match self {
            TeamMode::Multiplexed => "~2GB total, role switching via prompts",
            TeamMode::Isolated => "~10GB total, full isolation per role",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct SettingsModalProps {
    pub on_close: Callback<()>,
}

#[function_component(SettingsModal)]
pub fn settings_modal(props: &SettingsModalProps) -> Html {
    let settings = use_state(TeamSettings::default);
    let saving = use_state(|| false);
    let error = use_state(|| None::<String>);

    // Load settings on mount
    {
        let settings = settings.clone();
        let error = error.clone();
        use_effect(move || {
            let settings = settings.clone();
            let error = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_team_settings().await {
                    Ok(loaded) => settings.set(loaded),
                    Err(e) => error.set(Some(e)),
                }
            });
            || ()
        });
    }

    let on_mode_change = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            let new_mode = if input.checked() {
                TeamMode::Isolated
            } else {
                TeamMode::Multiplexed
            };
            let mut current = (*settings).clone();
            current.mode = new_mode;
            settings.set(current);
        })
    };

    let on_save = {
        let settings = settings.clone();
        let saving = saving.clone();
        let error = error.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_e: MouseEvent| {
            let settings = settings.clone();
            let saving = saving.clone();
            let error = error.clone();
            let on_close = on_close.clone();
            
            saving.set(true);
            error.set(None);
            
            wasm_bindgen_futures::spawn_local(async move {
                match save_team_settings(&*settings).await {
                    Ok(_) => {
                        saving.set(false);
                        on_close.emit(());
                    }
                    Err(e) => {
                        saving.set(false);
                        error.set(Some(e));
                    }
                }
            });
        })
    };

    let on_close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_e: MouseEvent| {
            on_close.emit(());
        })
    };

    let toggle_role = {
        let settings = settings.clone();
        Callback::from(move |role: String| {
            let mut current = (*settings).clone();
            if current.enabled_roles.contains(&role) {
                current.enabled_roles.retain(|r| r != &role);
            } else {
                current.enabled_roles.push(role);
            }
            settings.set(current);
        })
    };

    html! {
        <div class="modal-overlay" onclick={on_close_click.clone()}>
            <div class="modal" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="modal-header">
                    <h2>{"⚙️ Settings"}</h2>
                    <button class="btn-close" onclick={on_close_click}>{"×"}</button>
                </div>
                
                <div class="modal-body">
                    if let Some(e) = &*error {
                        <div class="error-message">{e}</div>
                    }
                    
                    <div class="settings-section">
                        <h3>{"👥 Team Agents Mode"}</h3>
                        <p class="settings-description">
                            {"Choose how team agents run. Affects memory usage and isolation."}
                        </p>
                        
                        <div class="mode-toggle">
                            <label class="toggle-option">
                                <input 
                                    type="checkbox" 
                                    checked={settings.mode == TeamMode::Isolated}
                                    onchange={on_mode_change}
                                />
                                <span class="toggle-label">
                                    <strong>{TeamMode::Isolated.display_name()}</strong>
                                    <span class="toggle-desc">{TeamMode::Isolated.description()}</span>
                                </span>
                            </label>
                            
                            <div class="mode-info">
                                <div class={if settings.mode == TeamMode::Multiplexed { "mode-badge active" } else { "mode-badge" }}>
                                    {"⚡ Multiplexed"}
                                </div>
                                <div class={if settings.mode == TeamMode::Isolated { "mode-badge active" } else { "mode-badge" }}>
                                    {"🔒 Isolated"}
                                </div>
                            </div>
                            
                            <p class="current-mode">
                                {"Current: "}<strong>{settings.mode.display_name()}</strong>
                                <br/>
                                <span class="mode-desc">{settings.mode.description()}</span>
                            </p>
                        </div>
                    </div>
                    
                    <div class="settings-section">
                        <h3>{"🎯 Enabled Roles"}</h3>
                        <p class="settings-description">
                            {"Select which team roles to create agents for."}
                        </p>
                        
                        <div class="roles-grid">
                            {for ALL_ROLES.iter().map(|(role, name, emoji)| {
                                let is_enabled = settings.enabled_roles.contains(&role.to_string());
                                let role_clone = role.to_string();
                                let on_toggle = toggle_role.clone();
                                html! {
                                    <label class="role-checkbox">
                                        <input 
                                            type="checkbox" 
                                            checked={is_enabled}
                                            onchange={Callback::from(move |_| on_toggle.emit(role_clone.clone()))}
                                        />
                                        <span>{emoji}{" "}{name}</span>
                                    </label>
                                }
                            })}
                        </div>
                    </div>
                </div>
                
                <div class="modal-footer">
                    <button 
                        class="btn-primary" 
                        onclick={on_save}
                        disabled={*saving}
                    >
                        {if *saving { "Saving..." } else { "Save Settings" }}
                    </button>
                </div>
            </div>
        </div>
    }
}

const ALL_ROLES: [(&str, &str, &str); 7] = [
    ("pm", "Project Manager", "📋"),
    ("developer", "Developer", "💻"),
    ("qa", "QA Engineer", "🔍"),
    ("designer", "Designer", "🎨"),
    ("devops", "DevOps", "🚀"),
    ("security", "Security", "🔒"),
    ("architect", "Architect", "🏛️"),
];

// API functions
async fn fetch_team_settings() -> Result<TeamSettings, String> {
    let token = api::get_token();
    let mut req = Request::get("http://localhost:8081/api/settings/team");
    
    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }
    
    let response = req
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 404 {
        // No settings saved yet, return defaults
        Ok(TeamSettings::default())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

async fn save_team_settings(settings: &TeamSettings) -> Result<(), String> {
    let token = api::get_token();
    let mut req = Request::post("http://localhost:8081/api/settings/team");
    
    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }
    
    let response = req
        .json(settings)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        Ok(())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}
