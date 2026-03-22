use crate::types::{AgentContainer, Team, TeamRoleAssignment, AssignRoleRequest};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const API_BASE: &str = "http://localhost:3001/api";

// === Auth Types ===

#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[allow(dead_code)]
    pub refresh_token: String,
    #[allow(dead_code)]
    pub token_type: String,
    #[allow(dead_code)]
    pub expires_in: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthStatus {
    pub auth_enabled: bool,
    pub has_admin: bool,
    #[allow(dead_code)]
    pub registration_enabled: bool,
}

// === Auth API ===

pub async fn get_auth_status() -> Result<AuthStatus, String> {
    let response = Request::get("http://localhost:3001/auth/status")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

pub async fn login(password: &str) -> Result<TokenResponse, String> {
    let response = Request::post("http://localhost:3001/auth/login")
        .json(&LoginRequest {
            password: password.to_string(),
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Login failed: {}", error_text))
    }
}

pub async fn register(password: &str) -> Result<(), String> {
    let response = Request::post("http://localhost:3001/auth/register")
        .json(&LoginRequest {
            password: password.to_string(),
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        Ok(())
    } else {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Registration failed: {}", error_text))
    }
}

// === Token Storage ===

pub fn store_token(token: &str) {
    let window = web_sys::window().unwrap();
    let storage = window.local_storage().unwrap().unwrap();
    storage.set_item("claw_pen_token", token).unwrap();
}

pub fn get_token() -> Option<String> {
    let window = web_sys::window().unwrap();
    let storage = window.local_storage().unwrap().unwrap();
    storage.get_item("claw_pen_token").unwrap()
}

pub fn clear_token() {
    let window = web_sys::window().unwrap();
    let storage = window.local_storage().unwrap().unwrap();
    storage.remove_item("claw_pen_token").unwrap();
}

// === Agent API (with auth) ===

pub async fn fetch_agents() -> Result<Vec<AgentContainer>, String> {
    let token = get_token();
    let mut req = Request::get(&format!("{}/agents", API_BASE));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

#[allow(dead_code)]
pub async fn start_agent(id: &str) -> Result<AgentContainer, String> {
    let token = get_token();
    let mut req = Request::post(&format!("{}/agents/{}/start", API_BASE, id));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

#[allow(dead_code)]
pub async fn stop_agent(id: &str) -> Result<AgentContainer, String> {
    let token = get_token();
    let mut req = Request::post(&format!("{}/agents/{}/stop", API_BASE, id));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

// === Teams API ===

pub async fn fetch_teams() -> Result<Vec<Team>, String> {
    let token = get_token();
    let mut req = Request::get(&format!("{}/teams", API_BASE));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

pub async fn fetch_team(id: &str) -> Result<Team, String> {
    let token = get_token();
    let mut req = Request::get(&format!("{}/teams/{}", API_BASE, id));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

pub async fn fetch_team_roles(team_id: &str) -> Result<Vec<TeamRoleAssignment>, String> {
    let token = get_token();
    let mut req = Request::get(&format!("{}/teams/{}/roles", API_BASE, team_id));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

pub async fn assign_team_role(
    team_id: &str,
    intent: &str,
    agent_id: &str,
    assigned_by: &str,
) -> Result<TeamRoleAssignment, String> {
    let token = get_token();
    let mut req = Request::post(&format!("{}/teams/{}/roles/{}", API_BASE, team_id, intent));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let request = AssignRoleRequest {
        agent_id: agent_id.to_string(),
        assigned_by: assigned_by.to_string(),
    };

    let response = req
        .json(&request)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

pub async fn remove_team_role(
    team_id: &str,
    intent: &str,
) -> Result<TeamRoleAssignment, String> {
    let token = get_token();
    let mut req = Request::delete(&format!("{}/teams/{}/roles/{}", API_BASE, team_id, intent));

    if let Some(ref t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else if response.status() == 401 {
        clear_token();
        Err("Authentication required".to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}
