use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::types::*;
use crate::container::RuntimeClient;
use crate::AppState;

pub async fn health() -> &'static str {
    "OK"
}

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AgentContainer>> {
    let containers = state.containers.read().await;
    Json(containers.clone())
}

pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Create container via Docker
    let id = state.runtime
        .create_container(&req.name, &req.config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let agent = AgentContainer {
        id: id.clone(),
        name: req.name,
        status: AgentStatus::Stopped,
        config: req.config,
        tailscale_ip: None,
        resource_usage: None,
    };

    let mut containers = state.containers.write().await;
    containers.push(agent.clone());
    
    Ok(Json(agent))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, StatusCode> {
    let containers = state.containers.read().await;
    
    containers
        .iter()
        .find(|a| a.id == id)
        .map(|a| Json(a.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentContainer>, StatusCode> {
    let mut containers = state.containers.write().await;
    
    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = req.name {
        agent.name = name;
    }
    
    if let Some(config_updates) = req.config {
        if let Some(provider) = config_updates.llm_provider {
            agent.config.llm_provider = provider;
        }
        if let Some(model) = config_updates.llm_model {
            agent.config.llm_model = Some(model);
        }
        if let Some(mem) = config_updates.memory_mb {
            agent.config.memory_mb = mem;
        }
        if let Some(cores) = config_updates.cpu_cores {
            agent.config.cpu_cores = cores;
        }
        if let Some(env) = config_updates.env_vars {
            agent.config.env_vars.extend(env);
        }
    }

    // TODO: Update container labels/env in Docker

    Ok(Json(agent.clone()))
}

pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Stop if running
    let _ = state.runtime.stop_container(&id).await;
    
    // Delete container
    state.runtime
        .delete_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let mut containers = state.containers.write().await;
    containers.retain(|a| a.id != id);
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    state.runtime
        .start_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let mut containers = state.containers.write().await;
    
    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    agent.status = AgentStatus::Running;
    
    Ok(Json(agent.clone()))
}

pub async fn stop_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    state.runtime
        .stop_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let mut containers = state.containers.write().await;
    
    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    agent.status = AgentStatus::Stopped;
    
    Ok(Json(agent.clone()))
}

pub async fn runtime_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "connected",
        "runtime": "docker",
        "deployment_mode": format!("{:?}", state.config.deployment_mode).to_lowercase(),
        "network_backend": format!("{:?}", state.config.network_backend).to_lowercase(),
        "runtime_socket": state.config.runtime_socket,
        "agents_running": state.containers.read().await.iter().filter(|a| a.status == AgentStatus::Running).count(),
        "model_servers": {
            "ollama": state.config.model_servers.ollama.as_ref().map(|s| s.endpoint.clone()),
            "llama_cpp": state.config.model_servers.llama_cpp.as_ref().map(|s| s.endpoint.clone()),
            "vllm": state.config.model_servers.vllm.as_ref().map(|s| s.endpoint.clone()),
        }
    }))
}
