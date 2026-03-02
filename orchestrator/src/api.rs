//! API handlers for Claw Pen Orchestrator
//!
//! # Authentication
//!
//! All endpoints except `/health`, `/auth/login`, `/auth/register`, and `/auth/status`
//! require JWT authentication via the `Authorization: Bearer <token>` header.
//!
//! WebSocket endpoints accept the JWT token via the `?token=<jwt>` query parameter.
//!
//! ## Getting a Token
//!
//! 1. First, set an admin password using the CLI: `claw-pen-orchestrator --set-password`
//!    OR enable registration with `ENABLE_REGISTRATION=true` and call POST /auth/register
//!
//! 2. Authenticate: `POST /auth/login` with `{"password": "your-password"}`
//!
//! 3. Use the returned `access_token` in subsequent requests:
//!    `Authorization: Bearer <access_token>`
//!
//! 4. Refresh tokens with `POST /auth/refresh` when the access token expires

use crate::validation;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::Serialize;

// Helper to sanitize error messages before returning to clients
fn sanitize_error(e: &str) -> String {
    validation::sanitize_error_message(e)
}
use std::collections::HashMap;
use std::sync::Arc;

use crate::andor;
use crate::container::ContainerRuntime;
use crate::types::*;
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

    let filtered: Vec<_> = containers
        .iter()
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
            // Filter by runtime
            if let Some(runtime) = params.get("runtime") {
                if c.runtime.as_deref() != Some(runtime.as_str()) {
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
    // === Input Validation ===

    // Validate agent name (container name)
    if let Err(e) = validation::validate_container_name(&req.name) {
        return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
    }

    // Validate project name if provided
    if let Some(ref project) = req.project {
        if let Err(e) = validation::validate_project_name(project) {
            return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
        }
    }

    // Validate tags if provided
    for tag in &req.tags {
        if let Err(e) = validation::validate_tag(tag) {
            return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
        }
    }

    // Validate runtime if provided
    let runtime = req.runtime.as_ref().map(|r| r.to_lowercase());
    if let Some(ref rt) = runtime {
        if rt != "docker" && rt != "exo" {
            return Err((StatusCode::BAD_REQUEST,
                format!("Invalid runtime '{}'. Must be 'docker' or 'exo'.", rt)));
        }
    }
    if let Some(ref cfg) = req.config {
        // Validate env vars count
        if let Some(ref env) = cfg.env_vars {
            if env.len() > validation::MAX_ENV_VARS_COUNT {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Too many environment variables (max {})",
                        validation::MAX_ENV_VARS_COUNT
                    ),
                ));
            }
            // Validate each env var key/value
            for (key, value) in env {
                if let Err(e) = validation::validate_env_key(key) {
                    return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
                }
                if let Err(e) = validation::validate_env_value(value) {
                    return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
                }
            }
        }

        // Validate secrets count
        if let Some(ref secrets) = cfg.secrets {
            if secrets.len() > validation::MAX_SECRETS_COUNT {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Too many secrets (max {})", validation::MAX_SECRETS_COUNT),
                ));
            }
            for secret in secrets {
                if let Err(e) = validation::validate_secret_name(secret) {
                    return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
                }
            }
        }

        // Validate volumes count and paths
        if let Some(ref volumes) = cfg.volumes {
            if volumes.len() > validation::MAX_VOLUMES_COUNT {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Too many volumes (max {})", validation::MAX_VOLUMES_COUNT),
                ));
            }
            for vol in volumes {
                // Note: Full path validation requires filesystem access, done at container creation
                if let Err(e) = validation::validate_container_target(&vol.target) {
                    return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
                }
            }
        }

        // Validate LLM model name if provided
        if let Some(ref model) = cfg.llm_model {
            if let Err(e) = validation::validate_llm_model(model) {
                return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
            }
        }

        // Validate memory and CPU if provided
        if let Some(mem) = cfg.memory_mb {
            if let Err(e) = validation::validate_memory_mb(mem) {
                return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
            }
        }
        if let Some(cpu) = cfg.cpu_cores {
            if let Err(e) = validation::validate_cpu_cores(cpu) {
                return Err((StatusCode::BAD_REQUEST, sanitize_error(&e.to_string())));
            }
        }
    }

    // === End Input Validation ===

    // Build config from template + overrides
    let mut config = if let Some(ref template_name) = req.template {
        state
            .templates
            .get(template_name)
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
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Template '{}' not found", template_name),
                )
            })?
    } else {
        AgentConfig::default()
    };

    // Apply overrides
    if let Some(ref partial) = req.config {
        config.apply(partial);
    }

    // Create container

    // Inject API key from agent config
    if let Some(ref key) = config.api_key {
        let key_var = match config.llm_provider {
            LlmProvider::Zai => "ZAI_API_KEY",
            LlmProvider::Anthropic => "ANTHROPIC_API_KEY",
            LlmProvider::OpenAI => "OPENAI_API_KEY",
            LlmProvider::Kimi => "KIMI_API_KEY",
            LlmProvider::KimiCode => "KIMI_CODE_API_KEY",
            LlmProvider::Gemini => "GOOGLE_API_KEY",
            LlmProvider::Access => "ACCESS_API_KEY",
            LlmProvider::Huggingface => "HF_TOKEN",
            _ => "API_KEY",
        };
        config.env_vars.insert(key_var.to_string(), key.clone());
    }

    // Determine which runtime to use
    // Priority: per-agent runtime > global config runtime
    let agent_runtime = runtime.or_else(|| {
        match state.config.container_runtime {
            crate::config::ContainerRuntimeType::Docker => Some("docker".to_string()),
            crate::config::ContainerRuntimeType::Exo => Some("exo".to_string()),
        }
    });

    // Get the appropriate runtime client based on agent's runtime preference
    let id = if let Some(ref rt) = agent_runtime {
        if rt == "exo" {
            // Use exo-specific runtime if available
            state
                .exo_runtime
                .create_container(&req.name, &config)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        } else {
            // Use default runtime (docker or containment)
            state
                .runtime
                .create_container(&req.name, &config)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
    } else {
        state
            .runtime
            .create_container(&req.name, &config)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

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
        runtime: agent_runtime,
    };

    // Register with AndOR Bridge if configured
    if let Some(ref andor) = state.andor {
        let should_register = state
            .config
            .andor_bridge
            .as_ref()
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

    // Persist to storage
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(&agent)) {
        tracing::warn!("Failed to persist agent: {}", e);
    }

    Ok(Json(agent))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    let containers = state.containers.read().await;
    containers
        .iter()
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
    let agent = containers
        .iter_mut()
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

    // Persist to storage
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
        tracing::warn!("Failed to persist agent update: {}", e);
    }

    Ok(Json(agent.clone()))
}

pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // First check if agent exists in our list and get its runtime
    let (agent_exists, agent_runtime) = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .map(|a| (true, a.runtime.clone()))
            .unwrap_or((false, None))
    };

    if !agent_exists {
        return Err((StatusCode::NOT_FOUND, "Agent not found".to_string()));
    }

    // Choose the right runtime based on agent's runtime setting
    let runtime: &dyn ContainerRuntime = if agent_runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    // Stop if running (ignore errors if container doesn't exist)
    let _ = runtime.stop_container(&id).await;

    // Delete container (ignore errors if container doesn't exist)
    let _ = runtime.delete_container(&id).await;

    // Unregister from AndOR Bridge
    if let Some(ref andor) = state.andor {
        if let Err(e) = andor.unregister_agent(&id).await {
            tracing::warn!("Failed to unregister from AndOR Bridge: {}", e);
        }
    }

    // Remove from state
    let mut containers = state.containers.write().await;
    containers.retain(|c| c.id != id);

    // Remove from storage
    if let Err(e) = crate::storage::remove_agent(&id) {
        tracing::warn!("Failed to remove agent from storage: {}", e);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    let mut containers = state.containers.write().await;

    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // Choose the right runtime based on agent's runtime setting
    let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    // Check if container exists, if not create it
    let container_exists = runtime.container_exists(&id).await.unwrap_or(false);

    if !container_exists {
        // Create the container for this stored agent

        // Inject API key from agent config
        if let Some(ref key) = agent.config.api_key {
            let key_var = match agent.config.llm_provider {
                LlmProvider::Zai => "ZAI_API_KEY",
                LlmProvider::Anthropic => "ANTHROPIC_API_KEY",
                LlmProvider::OpenAI => "OPENAI_API_KEY",
                LlmProvider::Kimi => "KIMI_API_KEY",
                LlmProvider::KimiCode => "KIMI_CODE_API_KEY",
                LlmProvider::Gemini => "GOOGLE_API_KEY",
                LlmProvider::Access => "ACCESS_API_KEY",
                LlmProvider::Huggingface => "HF_TOKEN",
                _ => "API_KEY",
            };
            agent
                .config
                .env_vars
                .insert(key_var.to_string(), key.clone());
        }
        let new_id = runtime
            .create_container(&agent.name, &agent.config)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Update the ID in case it changed
        if new_id != id {
            // ID mismatch - this shouldn't happen but handle it
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Container ID mismatch".to_string(),
            ));
        }
    }

    // Start the container
    runtime
        .start_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    agent.status = AgentStatus::Running;

    // Persist status change
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
        tracing::warn!("Failed to persist agent status: {}", e);
    }

    Ok(Json(agent.clone()))
}

pub async fn stop_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Get agent to find its runtime
    let agent_runtime = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.runtime.clone())
    };

    // Choose the right runtime
    let runtime: &dyn ContainerRuntime = if agent_runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    runtime
        .stop_container(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut containers = state.containers.write().await;

    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    agent.status = AgentStatus::Stopped;

    // Persist status change
    if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
        tracing::warn!("Failed to persist agent status: {}", e);
    }

    Ok(Json(agent.clone()))
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
            // Choose runtime based on agent's runtime setting
            let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
                &state.exo_runtime
            } else {
                &state.runtime
            };
            
            if runtime.start_container(&agent.id).await.is_ok() {
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
            // Choose runtime based on agent's runtime setting
            let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
                &state.exo_runtime
            } else {
                &state.runtime
            };
            
            if runtime.stop_container(&agent.id).await.is_ok() {
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
    // Get agent to find its runtime
    let agent_runtime = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.runtime.clone())
    };

    // Choose the right runtime
    let runtime: &dyn ContainerRuntime = if agent_runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    let tail: usize = params
        .get("tail")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let logs = runtime
        .get_logs(&id, tail)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(logs))
}

pub async fn logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    // Validate JWT token from query parameter
    let token = params.get("token").ok_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication token".to_string(),
    ))?;

    let auth = state.auth.read().await;
    auth.validate_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e)))?;
    drop(auth);

    // Check if agent exists
    let containers = state.containers.read().await;
    let _agent = containers
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;
    drop(containers);

    Ok(ws.on_upgrade(move |socket| handle_logs_stream(socket, state, id)))
}

async fn handle_logs_stream(mut socket: WebSocket, state: Arc<AppState>, id: String) {
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
    // Get agent to find its runtime
    let agent_runtime = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.runtime.clone())
    };

    // Choose the right runtime
    let runtime: &dyn ContainerRuntime = if agent_runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    let usage = runtime
        .get_stats(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Agent not found or not running".to_string(),
            )
        })?;

    Ok(Json(usage))
}

pub async fn get_all_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<HashMap<String, ResourceUsage>> {
    let containers = state.containers.read().await;
    let mut metrics = HashMap::new();

    for agent in containers.iter() {
        if agent.status == AgentStatus::Running {
            // Choose runtime based on agent's runtime setting
            let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
                &state.exo_runtime
            } else {
                &state.runtime
            };
            
            if let Ok(Some(usage)) = runtime.get_stats(&agent.id).await {
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
    // Get agent to find its runtime
    let agent_runtime = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.runtime.clone())
    };

    // Choose the right runtime
    let runtime: &dyn ContainerRuntime = if agent_runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    let healthy = runtime
        .health_check(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = HealthStatus {
        healthy,
        last_check: chrono::Utc::now().to_rfc3339(),
        message: if healthy {
            Some("OK".to_string())
        } else {
            Some("Health check failed".to_string())
        },
    };

    // Update agent status
    let mut containers = state.containers.write().await;
    if let Some(agent) = containers.iter_mut().find(|c| c.id == id) {
        agent.health_status = Some(status.clone());
    }

    Ok(Json(status))
}

// === System Stats ===

#[derive(Debug, serde::Serialize)]
pub struct SystemStats {
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub available_memory_mb: u64,
    pub total_cpu_cores: f32,
    pub cpu_usage_percent: f32,
    pub agent_count: usize,
    pub running_agents: usize,
    pub agent_memory_mb: u64,
    pub runtime: String,
}

pub async fn get_system_stats(State(state): State<Arc<AppState>>) -> Json<SystemStats> {
    let containers = state.containers.read().await;

    let running: Vec<_> = containers
        .iter()
        .filter(|a| a.status == AgentStatus::Running)
        .collect();
    let agent_memory: u64 = running.iter().map(|a| a.config.memory_mb as u64).sum();

    // Get actual system memory from /proc/meminfo
    let (total_mem, available_mem) = get_system_memory();
    let used_mem = total_mem.saturating_sub(available_mem);

    // Get CPU cores
    let cpu_cores = num_cpus::get() as f32;

    // Get CPU usage (simplified - just count running containers)
    let cpu_usage = (running.len() as f32 / cpu_cores.max(1.0)) * 100.0;

    // Determine active runtime
    let runtime = match state.config.container_runtime {
        crate::config::ContainerRuntimeType::Docker => "docker",
        crate::config::ContainerRuntimeType::Exo => "exo",
    };

    Json(SystemStats {
        total_memory_mb: total_mem / 1024,
        used_memory_mb: used_mem / 1024,
        available_memory_mb: available_mem / 1024,
        total_cpu_cores: cpu_cores,
        cpu_usage_percent: cpu_usage.min(100.0),
        agent_count: containers.len(),
        running_agents: running.len(),
        agent_memory_mb: agent_memory,
        runtime: runtime.to_string(),
    })
}

fn get_system_memory() -> (u64, u64) {
    use std::fs;

    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        let mut total = 0u64;
        let mut available = 0u64;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
            } else if line.starts_with("MemAvailable:") {
                available = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }

        (total, available)
    } else {
        (8192 * 1024, 4096 * 1024) // Fallback: 8GB total, 4GB available
    }
}

// === Templates ===

pub async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<(String, TemplateInfo)>> {
    let templates: Vec<_> = state
        .templates
        .list()
        .into_iter()
        .map(|(name, t)| {
            (
                name.clone(),
                TemplateInfo {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    provider: t.config.llm_provider.clone(),
                    model: t.config.llm_model.clone(),
                },
            )
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

pub async fn list_projects(State(state): State<Arc<AppState>>) -> Json<Vec<Project>> {
    let containers = state.containers.read().await;
    let mut projects: HashMap<String, Project> = HashMap::new();

    for agent in containers.iter() {
        if let Some(ref project_name) = agent.project {
            let project = projects
                .entry(project_name.clone())
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
    State(_state): State<Arc<AppState>>,
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
    let secrets = state.secrets.list_secrets(&id).await.unwrap_or_default();

    Json(secrets)
}

pub async fn set_secret(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SetSecretRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .secrets
        .set_secret(&id, &req.name, &req.value)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::CREATED)
}

pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Path((id, name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .secrets
        .delete_secret(&id, &name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// === API Keys ===

#[derive(Debug, serde::Deserialize)]
pub struct SetApiKeyRequest {
    pub provider: String,
    pub key: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ApiKeyInfo {
    pub provider: String,
    pub has_key: bool,
}

pub async fn list_api_keys(State(state): State<Arc<AppState>>) -> Json<Vec<ApiKeyInfo>> {
    let keys = state.api_keys.read().await;
    let providers = [
        "zai",
        "anthropic",
        "openai",
        "kimi",
        "google",
        "kimi-code",
        "access",
        "huggingface",
    ];

    Json(
        providers
            .iter()
            .map(|p| ApiKeyInfo {
                provider: p.to_string(),
                has_key: keys.contains_key(*p),
            })
            .collect(),
    )
}

pub async fn set_api_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetApiKeyRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut keys = state.api_keys.write().await;
    keys.insert(req.provider.clone(), req.key);

    // Persist to disk
    let keys_path = state.data_dir.join("api_keys.json");
    if let Ok(json) = serde_json::to_string_pretty(&*keys) {
        let _ = std::fs::write(&keys_path, json);
    }

    Ok(StatusCode::CREATED)
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut keys = state.api_keys.write().await;
    keys.remove(&provider);

    // Persist to disk
    let keys_path = state.data_dir.join("api_keys.json");
    if let Ok(json) = serde_json::to_string_pretty(&*keys) {
        let _ = std::fs::write(&keys_path, json);
    }

    Ok(StatusCode::NO_CONTENT)
}

// === Snapshots ===

pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<SnapshotInfo>> {
    let snapshots = state
        .snapshots
        .list_snapshots(&id)
        .await
        .unwrap_or_default();

    Json(snapshots)
}

pub async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SnapshotInfo>, (StatusCode, String)> {
    let snapshot = state
        .snapshots
        .create_snapshot(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(snapshot))
}

pub async fn restore_snapshot(
    State(state): State<Arc<AppState>>,
    Path((id, snapshot_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .snapshots
        .restore_snapshot(&id, &snapshot_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn delete_snapshot(
    State(state): State<Arc<AppState>>,
    Path((id, snapshot_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .snapshots
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
    let config = state
        .snapshots
        .export_agent(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"agent-{}.json\"", id),
        )
        .body(Body::from(config))
        .unwrap())
}

pub async fn import_agent(
    State(state): State<Arc<AppState>>,
    Json(agent): Json<AgentContainer>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Choose runtime based on imported agent's runtime setting
    let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    // Create the container
    let id = runtime
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

pub async fn runtime_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let runtime_name = match state.config.container_runtime {
        crate::config::ContainerRuntimeType::Docker => "docker",
        crate::config::ContainerRuntimeType::Exo => "exo",
    };

    Json(serde_json::json!({
        "runtime": runtime_name,
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
        "kimi" => LlmProvider::Kimi,
        "zai" => LlmProvider::Zai,
        "huggingface" => LlmProvider::Huggingface,
        "ollama" => LlmProvider::Ollama,
        "llamacpp" => LlmProvider::LlamaCpp,
        "vllm" => LlmProvider::Vllm,
        "lmstudio" => LlmProvider::Lmstudio,
        _ => LlmProvider::OpenAI,
    }
}

// === Chat WebSocket ===

/// WebSocket endpoint for agent chat
///
/// Authentication: Pass JWT token via `?token=<jwt>` query parameter
///
/// Example: `ws://localhost:3000/api/agents/{id}/chat?token=eyJhbGciOiJIUzI1NiIs...`
pub async fn chat_websocket(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    // Validate JWT token from query parameter
    let token = params.get("token").ok_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication token".to_string(),
    ))?;

    let auth = state.auth.read().await;
    let _claims = auth
        .validate_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e)))?;
    drop(auth);

    // Check if agent exists and is running
    let containers = state.containers.read().await;
    let agent = containers
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    if agent.status != AgentStatus::Running {
        return Err((StatusCode::BAD_REQUEST, "Agent is not running".to_string()));
    }

    let agent_id = agent.id.clone();
    drop(containers);

    Ok(ws.on_upgrade(move |socket| handle_chat_stream(socket, state, agent_id)))
}

async fn handle_chat_stream(socket: WebSocket, _state: Arc<AppState>, _agent_id: String) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    let (mut tx, mut rx) = socket.split();

    // Send welcome message
    let welcome = serde_json::json!({
        "role": "system",
        "content": "Connected to agent. Send a message to start chatting.",
        "timestamp": chrono::Utc::now().timestamp()
    });

    if tx.send(Message::Text(welcome.to_string())).await.is_err() {
        return;
    }

    // Handle incoming messages
    while let Some(msg_result) = rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                // Parse the incoming message
                if let Ok(msg_data) = serde_json::from_str::<serde_json::Value>(&text) {
                    let user_content = msg_data
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or(&text);

                    // TODO: Forward to actual agent container via its own WebSocket/API
                    // For now, echo back with a placeholder response
                    let response = serde_json::json!({
                        "role": "assistant",
                        "content": format!("Echo: {}", user_content),
                        "timestamp": chrono::Utc::now().timestamp()
                    });

                    if tx.send(Message::Text(response.to_string())).await.is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(_) => {
                break;
            }
            _ => {}
        }
    }
}

// === Teams ===

pub async fn list_teams(State(state): State<Arc<AppState>>) -> Json<Vec<crate::types::Team>> {
    Json(state.teams.list().await)
}

pub async fn get_team(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<crate::types::Team>, (StatusCode, String)> {
    state
        .teams
        .get(&id)
        .await
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Team not found".to_string()))
}

/// Classify a message to determine which agent should handle it
pub async fn classify_message(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ClassifyRequest>,
) -> Result<Json<ClassificationResult>, (StatusCode, String)> {
    let team = state
        .teams
        .get(&id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Team not found".to_string()))?;

    let router = crate::teams::Router::new(team);
    let result = router.classify(&req.message);

    Ok(Json(result))
}

#[derive(Debug, serde::Deserialize)]
pub struct ClassifyRequest {
    pub message: String,
}

/// WebSocket endpoint for team chat with routing
///
/// Authentication: Pass JWT token via `?token=<jwt>` query parameter
pub async fn team_chat_websocket(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    // Validate JWT token from query parameter
    let token = params.get("token").ok_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication token".to_string(),
    ))?;

    let auth = state.auth.read().await;
    auth.validate_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e)))?;
    drop(auth);

    // Check if team exists
    let team = state
        .teams
        .get(&id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Team not found".to_string()))?;

    let team_id = team.id.clone();
    let team_name = team.name.clone();

    Ok(ws.on_upgrade(move |socket| handle_team_chat_stream(socket, state, team_id, team_name)))
}

async fn handle_team_chat_stream(
    socket: WebSocket,
    state: Arc<AppState>,
    team_id: String,
    team_name: String,
) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    let (mut tx, mut rx) = socket.split();

    // Send welcome message
    let welcome = serde_json::json!({
        "role": "system",
        "content": format!("Connected to {} team. I'll route your message to the right specialist.", team_name),
        "timestamp": chrono::Utc::now().timestamp()
    });

    if tx.send(Message::Text(welcome.to_string())).await.is_err() {
        return;
    }

    // Handle incoming messages
    while let Some(msg_result) = rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                if let Ok(msg_data) = serde_json::from_str::<serde_json::Value>(&text) {
                    let user_content = msg_data
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or(&text);

                    // Get team and classify message
                    let response = if let Some(team) = state.teams.get(&team_id).await {
                        let router = crate::teams::Router::new(team.clone());
                        let classification = router.classify(user_content);

                        if classification.needs_clarification {
                            // Ask for clarification
                            let clarification = router.generate_clarification();
                            serde_json::json!({
                                "role": "assistant",
                                "content": clarification,
                                "classification": classification,
                                "timestamp": chrono::Utc::now().timestamp()
                            })
                        } else if let Some(agent) = router.get_target_agent(&classification) {
                            // Route to agent
                            let ack = router.get_routing_ack(&agent.description);

                            // Send routing acknowledgment
                            let ack_msg = serde_json::json!({
                                "role": "assistant",
                                "content": ack,
                                "classification": classification.clone(),
                                "routing_to": agent.agent,
                                "timestamp": chrono::Utc::now().timestamp()
                            });

                            if tx.send(Message::Text(ack_msg.to_string())).await.is_err() {
                                break;
                            }

                            // TODO: Forward message to actual agent and get response
                            // For now, return a placeholder
                            let agent_response = format!(
                                "[{}] I received your message: \"{}\"\n\n(Forwarding to {} container...)",
                                agent.description, user_content, agent.agent
                            );

                            serde_json::json!({
                                "role": "assistant",
                                "content": agent_response,
                                "from_agent": agent.agent,
                                "classification": classification,
                                "timestamp": chrono::Utc::now().timestamp()
                            })
                        } else {
                            // No matching agent found
                            serde_json::json!({
                                "role": "assistant",
                                "content": "I couldn't determine which specialist to route your message to. Please try rephrasing.",
                                "classification": classification,
                                "timestamp": chrono::Utc::now().timestamp()
                            })
                        }
                    } else {
                        serde_json::json!({
                            "role": "assistant",
                            "content": "Team configuration not found.",
                            "timestamp": chrono::Utc::now().timestamp()
                        })
                    };

                    if tx.send(Message::Text(response.to_string())).await.is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(_) => {
                break;
            }
            _ => {}
        }
    }
}
