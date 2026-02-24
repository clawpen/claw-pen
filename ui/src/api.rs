use crate::types::AgentContainer;
use gloo_net::http::Request;

const API_BASE: &str = "http://localhost:3000/api";

pub async fn fetch_agents() -> Result<Vec<AgentContainer>, String> {
    let response = Request::get(&format!("{}/agents", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

#[allow(dead_code)]
pub async fn start_agent(id: &str) -> Result<AgentContainer, String> {
    let response = Request::post(&format!("{}/agents/{}/start", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}

#[allow(dead_code)]
pub async fn stop_agent(id: &str) -> Result<AgentContainer, String> {
    let response = Request::post(&format!("{}/agents/{}/stop", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("API error: {}", response.status()))
    }
}
