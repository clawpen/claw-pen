use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
    body::Body,
    response::Response,
};
use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};
use std::sync::Arc;
use std::collections::HashMap;
use serde::Serialize;

use crate::types::*;
use crate::container::ContainerRuntime;
use crate::andor;
use crate::AppState;

// === Health ===

pub async fn health() -> &'static str {
    "OK"
}

// === Agents ===

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<AgentContainer>> {
    let containers = state.containers.read().await;
    
    let filtered: Vec<_> = containers.iter()
        .filter(|c| {
            // Filter by project
            if let Some(project) = params.get("project") {
                if c.project.as_deref() != Some(project.as_str()) {
                    return false;
                }
            }
            // Filter by status
            if let Some(status) = params.get("status") {
                if format!("{:?}", c.status).to_lowercase() != status.to_lowercase() {
                    return false;
                }
            }
            // Filter by tag
            if let Some(tag) = params.get("tag") {
                if !c.tags.contains(tag) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    Json(filtered)
}

pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Build config from template + overrides
    let mut config = if let Some(ref template_name) = req.template {
        state.templates.get(template_name)
            .map(|t| {
                let mut cfg = AgentConfig::default();
                if let Some(ref provider) = t.config.llm_provider {
                    cfg.llm_provider = parse_provider(provider);
                }
                if let Some(ref model) = t.config.llm_model {
                    cfg.llm_model = Some(model.clone());
                }
                cfg.memory_mb = t.config.memory_mb;
                cfg.cpu_cores = t.config.cpu_cores;
                cfg.env_vars = t.env.clone();
                cfg
            })
            .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("Template '{}' not found", template_name)))?
    } else {
        AgentConfig::default()
    };

    // Apply overrides
    if let Some(ref partial) = req.config {
        config.apply(partial);
    }

    // Create container
    let id = state.runtime
        .create_container(&req.name, &config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent = AgentContainer {
        id,
        name: req.name,
        status: AgentStatus::Stopped,
        config,
        tailscale_ip: None,
        resource_usage: None,
        project: req.project,
        tags: req.tags,
        restart_policy: AgentConfig::default().restart_policy,
        health_status: None,
    };

    // Register with AndOR Bridge if configured
    if let Some(ref andor) = state.andor {
        let should_register = state.config.andor_bridge.as_ref()
            .and_then(|c| c.register_on_create)
            .unwrap_or(false);
        
        if should_register {
            let registration = andor::AgentRegistration {
                agent_id: agent.id.clone(),
                display_name: agent.name.clone(),
                triggers: vec![agent.name.to_lowercase()],
                emoji: None,
            };
            if let Err(e) = andor.register_agent(&registration).await {
                tracing::warn!("Failed to register with AndOR Bridge: {}", e);
            }
        }
    }

    // Add to state
    let mut containers = state.containers.write().await;
    containers.push(agent.clone());

    Ok(Json(agent))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    let containers = state.containers.read().await;
    containers.iter()
        .find(|c| c.id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))
}

pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    let mut containers = state.containers.write().await;
    let agent = containers.iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    if let Some(name) = req.name {
        agent.name = name;
    }
    if let Some(project) = req.project {
        agent.project = Some(project);
    }
    if let Some(tags) = req.tags {
        agent.tags = tags;
    }
    if let Some(ref partial) = req.config {
        agent.config.apply(partial);
    }

    Ok(Json(agent.clone()))
}

pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Stop and delete container
    state.runtime
        .delete_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Unregister from AndOR Bridge
    if let Some(ref andor) = state.andor {
        if let Err(e) = andor.unregister_agent(&id).await {
            tracing::warn!("Failed to unregister from AndOR Bridge: {}", e);
        }
    }

    // Remove from state
    let mut containers = state.containers.write().await;
    containers.retain(|c| c.id != id);

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
    if let Some(agent) = containers.iter_mut().find(|c| c.id == id) {
        agent.status = AgentStatus::Running;
        Ok(Json(agent.clone()))
    } else {
        Err((StatusCode::NOT_FOUND, "Agent not found".to_string()))
    }
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
    if let Some(agent) = containers.iter_mut().find(|c| c.id == id) {
        agent.status = AgentStatus::Stopped;
        Ok(Json(agent.clone()))
    } else {
        Err((StatusCode::NOT_FOUND, "Agent not found".to_string()))
    }
}

// === Batch Operations ===

pub async fn start_all(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<String>> {
    let containers = state.containers.read().await;
    let mut started = Vec::new();

    for agent in containers.iter() {
        // Filter by project if specified
        if let Some(project) = params.get("project") {
            if agent.project.as_deref() != Some(project.as_str()) {
                continue;
            }
        }

        if agent.status != AgentStatus::Running {
            if state.runtime.start_container(&agent.id).await.is_ok() {
                started.push(agent.id.clone());
            }
        }
    }

    Json(started)
}

pub async fn stop_all(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<String>> {
    let containers = state.containers.read().await;
    let mut stopped = Vec::new();

    for agent in containers.iter() {
        if let Some(project) = params.get("project") {
            if agent.project.as_deref() != Some(project.as_str()) {
                continue;
            }
        }

        if agent.status == AgentStatus::Running {
            if state.runtime.stop_container(&agent.id).await.is_ok() {
                stopped.push(agent.id.clone());
            }
        }
    }

    Json(stopped)
}

// === Logs ===

pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, String)> {
    let tail: usize = params.get("tail")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let logs = state.runtime
        .get_logs(&id, tail)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(logs))
}

pub async fn logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_logs_stream(socket, state, id))
}

async fn handle_logs_stream(socket: WebSocket, state: Arc<AppState>, id: String) {
    use axum::extract::ws::Message;
    use tokio_stream::StreamExt;

    let mut stream = state.runtime.stream_logs(&id).await;
    
    while let Some(log) = stream.next().await {
        let msg = serde_json::to_string(&log).unwrap_or_default();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}

// === Metrics ===

pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ResourceUsage>, (StatusCode, String)> {
    let usage = state.runtime
        .get_stats(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found or not running".to_string()))?;

    Ok(Json(usage))
}

pub async fn get_all_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<HashMap<String, ResourceUsage>> {
    let containers = state.containers.read().await;
    let mut metrics = HashMap::new();

    for agent in containers.iter() {
        if agent.status == AgentStatus::Running {
            if let Ok(Some(usage)) = state.runtime.get_stats(&agent.id).await {
                metrics.insert(agent.id.clone(), usage);
            }
        }
    }

    Json(metrics)
}

// === Health Checks ===

pub async fn run_health_check(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<HealthStatus>, (StatusCode, String)> {
    let healthy = state.runtime
        .health_check(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = HealthStatus {
        healthy,
        last_check: chrono::Utc::now().to_rfc3339(),
        message: if healthy { Some("OK".to_string()) } else { Some("Health check failed".to_string()) },
    };

    // Update agent status
    let mut containers = state.containers.write().await;
    if let Some(agent) = containers.iter_mut().find(|c| c.id == id) {
        agent.health_status = Some(status.clone());
    }

    Ok(Json(status))
}

// === Templates ===

pub async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<(String, TemplateInfo)>> {
    let templates: Vec<_> = state.templates.list()
        .into_iter()
        .map(|(name, t)| {
            (name.clone(), TemplateInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                provider: t.config.llm_provider.clone(),
                model: t.config.llm_model.clone(),
            })
        })
        .collect();

    Json(templates)
}

#[derive(Serialize)]
pub struct TemplateInfo {
    pub name: String,
    pub description: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

// === Projects ===

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Project>> {
    let containers = state.containers.read().await;
    let mut projects: HashMap<String, Project> = HashMap::new();

    for agent in containers.iter() {
        if let Some(ref project_name) = agent.project {
            let project = projects.entry(project_name.clone())
                .or_insert_with(|| Project {
                    id: project_name.to_lowercase().replace(' ', "-"),
                    name: project_name.clone(),
                    description: None,
                    agents: Vec::new(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
            project.agents.push(agent.id.clone());
        }
    }

    Json(projects.into_values().collect())
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Json<Project> {
    let project = Project {
        id: req.name.to_lowercase().replace(' ', "-"),
        name: req.name,
        description: req.description,
        agents: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    Json(project)
}

// === Secrets ===

pub async fn list_secrets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<SecretInfo>> {
    let secrets = state.secrets
        .list_secrets(&id)
        .await
        .unwrap_or_default();

    Json(secrets)
}

pub async fn set_secret(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SetSecretRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.secrets
        .set_secret(&id, &req.name, &req.value)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::CREATED)
}

pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Path((id, name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.secrets
        .delete_secret(&id, &name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// === Snapshots ===

pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<SnapshotInfo>> {
    let snapshots = state.snapshots
        .list_snapshots(&id)
        .await
        .unwrap_or_default();

    Json(snapshots)
}

pub async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SnapshotInfo>, (StatusCode, String)> {
    let snapshot = state.snapshots
        .create_snapshot(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(snapshot))
}

pub async fn restore_snapshot(
    State(state): State<Arc<AppState>>,
    Path((id, snapshot_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.snapshots
        .restore_snapshot(&id, &snapshot_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn delete_snapshot(
    State(state): State<Arc<AppState>>,
    Path((id, snapshot_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.snapshots
        .delete_snapshot(&id, &snapshot_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// === Export/Import ===

pub async fn export_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let config = state.snapshots
        .export_agent(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Content-Disposition", format!("attachment; filename=\"agent-{}.json\"", id))
        .body(Body::from(config))
        .unwrap())
}

pub async fn import_agent(
    State(state): State<Arc<AppState>>,
    Json(agent): Json<AgentContainer>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Create the container
    let id = state.runtime
        .create_container(&agent.name, &agent.config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent = agent;
    agent.id = id;

    // Add to state
    let mut containers = state.containers.write().await;
    containers.push(agent.clone());

    Ok(Json(agent))
}

// === Runtime Status ===

pub async fn runtime_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "runtime": "containment",
        "version": env!("CARGO_PKG_VERSION"),
        "agents": {
            "total": state.containers.read().await.len(),
            "running": state.containers.read().await.iter().filter(|c| c.status == AgentStatus::Running).count(),
        }
    }))
}

// === Helpers ===

fn parse_provider(s: &str) -> LlmProvider {
    match s.to_lowercase().as_str() {
        "openai" => LlmProvider::OpenAI,
        "anthropic" => LlmProvider::Anthropic,
        "gemini" => LlmProvider::Gemini,
        "groq" => LlmProvider::Groq,
        "kimi" => LlmProvider::Kimi,
        "zai" => LlmProvider::Zai,
        "ollama" => LlmProvider::Ollama,
        "llamacpp" => LlmProvider::LlamaCpp,
        "vllm" => LlmProvider::Vllm,
        "lmstudio" => LlmProvider::Lmstudio,
        _ => LlmProvider::OpenAI,
    }
}
