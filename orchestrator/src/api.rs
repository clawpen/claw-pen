use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::types::*;
use crate::container::ContainerRuntime;
use crate::AppState;

pub async fn health() -> &'static str {
    "OK"
}

pub async fn list_agents(State(state): State<Arc<AppState>>) -> Json<Vec<AgentContainer>> {
    let containers = state.containers.read().await;
    Json(containers.clone())
}

pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Build config from template + overrides
    let config = if let Some(ref template_name) = req.template {
        let template = state.templates.get(template_name).ok_or((
            StatusCode::BAD_REQUEST,
            format!("Template '{}' not found", template_name),
        ))?;

        // Parse provider from template
        let provider = template
            .config
            .llm_provider
            .as_ref()
            .map(|s| parse_provider(s))
            .transpose()
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?
            .unwrap_or_default();

        let mut config = AgentConfig {
            llm_provider: provider,
            llm_model: template.config.llm_model.clone(),
            memory_mb: template.config.memory_mb,
            cpu_cores: template.config.cpu_cores,
            env_vars: template.env.clone(),
        };

        // Apply user overrides
        if let Some(ref partial) = req.config {
            config.apply(partial);
        }

        config
    } else {
        // No template - require full config
        let partial = req.config.ok_or((
            StatusCode::BAD_REQUEST,
            "Either 'template' or 'config' is required".to_string(),
        ))?;

        AgentConfig {
            llm_provider: partial.llm_provider.unwrap_or_default(),
            llm_model: partial.llm_model,
            memory_mb: partial.memory_mb.unwrap_or(1024),
            cpu_cores: partial.cpu_cores.unwrap_or(1.0),
            env_vars: partial.env_vars.unwrap_or_default(),
        }
    };

    // Create container via Docker
    let id = state
        .runtime
        .create_container(&req.name, &config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent = AgentContainer {
        id: id.clone(),
        name: req.name.clone(),
        status: AgentStatus::Stopped,
        config,
        tailscale_ip: None,
        resource_usage: None,
    };

    // Register with AndOR Bridge if configured
    if let Some(ref andor) = state.andor {
        let registration = crate::andor::AgentRegistration {
            agent_id: id.clone(),
            display_name: req.name.clone(),
            triggers: vec![req.name.to_lowercase()],
            emoji: Some("🤖".to_string()),
        };

        if let Err(e) = andor.register_agent(&registration).await {
            tracing::warn!("Failed to register agent with AndOR Bridge: {}", e);
        }
    }

    let mut containers = state.containers.write().await;
    containers.push(agent.clone());

    Ok(Json(agent))
}

fn parse_provider(s: &str) -> Result<LlmProvider, String> {
    match s.to_lowercase().as_str() {
        "openai" => Ok(LlmProvider::OpenAI),
        "anthropic" => Ok(LlmProvider::Anthropic),
        "gemini" => Ok(LlmProvider::Gemini),
        "groq" => Ok(LlmProvider::Groq),
        "ollama" => Ok(LlmProvider::Ollama),
        "llamacpp" | "llama.cpp" => Ok(LlmProvider::LlamaCpp),
        "vllm" => Ok(LlmProvider::Vllm),
        "lmstudio" | "lm-studio" => Ok(LlmProvider::LmStudio),
        _ => Err(format!("Unknown provider: {}", s)),
    }
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
    state
        .runtime
        .delete_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Unregister from AndOR Bridge if configured
    if let Some(ref andor) = state.andor {
        if let Err(e) = andor.unregister_agent(&id).await {
            tracing::warn!("Failed to unregister agent from AndOR Bridge: {}", e);
        }
    }

    let mut containers = state.containers.write().await;
    containers.retain(|a| a.id != id);

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    state
        .runtime
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
    state
        .runtime
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

pub async fn runtime_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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
            "lm_studio": state.config.model_servers.lm_studio.as_ref().map(|s| s.endpoint.clone()),
        },
        "andor_bridge": state.config.andor_bridge.as_ref().map(|c| serde_json::json!({
            "url": c.url,
            "register_on_create": c.register_on_create.unwrap_or(true)
        }))
    }))
}

pub async fn list_templates(State(state): State<Arc<AppState>>) -> Json<Vec<serde_json::Value>> {
    let templates: Vec<_> = state
        .templates
        .list()
        .into_iter()
        .map(|(id, t)| {
            serde_json::json!({
                "id": id,
                "name": t.name,
                "description": t.description,
                "defaults": {
                    "llm_provider": t.config.llm_provider,
                    "llm_model": t.config.llm_model,
                    "memory_mb": t.config.memory_mb,
                    "cpu_cores": t.config.cpu_cores,
                }
            })
        })
        .collect();

    Json(templates)
}
