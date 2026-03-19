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

use crate::storage;
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

// Re-export volume attachment functions
pub use crate::volume_attachment::{
    list_agent_volumes, attach_volume_to_agent, detach_volume_from_agent,
};

/// Base port for agent gateways
const BASE_AGENT_PORT: u16 = 18790;
/// Maximum port to allocate (gives us 10 agents: 18790-18799)
const MAX_AGENT_PORT: u16 = 18799;

/// Find the next available port for a new agent
fn allocate_port(existing_agents: &[AgentContainer]) -> u16 {
    let used_ports: std::collections::HashSet<u16> =
        existing_agents.iter().map(|a| a.gateway_port).collect();

    for port in BASE_AGENT_PORT..=MAX_AGENT_PORT {
        if !used_ports.contains(&port) {
            return port;
        }
    }

    // If all ports in range are used, extend beyond (shouldn't happen often)
    for port in (MAX_AGENT_PORT + 1)..=(MAX_AGENT_PORT + 100) {
        if !used_ports.contains(&port) {
            return port;
        }
    }

    // Fallback (should never reach here)
    MAX_AGENT_PORT + 1
}

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
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid runtime '{}'. Must be 'docker' or 'exo'.", rt),
            ));
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
                cfg.image = t.config.image.clone();

                // Create safe volume mounts from template
                // Host paths will be under {data_dir}/agents/{agent_name}/
                if !t.config.volumes.is_empty() {
                    // Get current working directory for absolute paths
                    let cwd = std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."));

                    // Create agent-specific data directory (absolute path)
                    let agent_data_dir = cwd
                        .join("data")
                        .join("agents")
                        .join(&req.name)
                        .join("volumes");

                    // Ensure the directory exists
                    if let Err(e) = std::fs::create_dir_all(&agent_data_dir) {
                        tracing::warn!("Failed to create agent data directory: {}", e);
                    } else {
                        tracing::info!("Created agent data directory: {:?}", agent_data_dir);
                    }

                    // Convert volume paths to VolumeMounts
                    for volume_path in &t.config.volumes {
                        // Extract the last component as the directory name
                        // e.g., "/agent/memory" -> "memory"
                        let dir_name = volume_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(volume_path)
                            .to_string();

                        // Create host path (absolute)
                        let host_path = agent_data_dir.join(&dir_name);

                        // Create host directory
                        if let Err(e) = std::fs::create_dir_all(&host_path) {
                            tracing::warn!("Failed to create volume directory: {:?}: {}", host_path, e);
                        } else {
                            tracing::info!("Created volume directory: {:?}", host_path);
                        }

                        // Convert to absolute path string for Docker
                        // Keep the native Windows path format (backslashes)
                        // Docker Desktop for Windows handles this correctly
                        let absolute_path = host_path
                            .canonicalize()
                            .unwrap_or(host_path)
                            .to_string_lossy()
                            .to_string();

                        cfg.volumes.push(crate::types::VolumeMount {
                            source: absolute_path,
                            target: volume_path.clone(),
                            read_only: false,
                        });
                    }
                }

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

    // Allocate a port for this agent
    let containers = state.containers.read().await;
    let gateway_port = allocate_port(&containers);
    drop(containers); // Release lock before continuing

    // Add port to environment variables
    config
        .env_vars
        .insert("PORT".to_string(), gateway_port.to_string());

    // Inject API key from agent config, or from global stored keys
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

    // First check agent-specific key, then fall back to globally stored key
    let stored_keys = state.api_keys.read().await;
    let provider_name = match config.llm_provider {
        LlmProvider::Zai => "zai",
        LlmProvider::Anthropic => "anthropic",
        LlmProvider::OpenAI => "openai",
        LlmProvider::Kimi => "kimi",
        LlmProvider::KimiCode => "kimi-code",
        LlmProvider::Gemini => "google",
        LlmProvider::Access => "access",
        LlmProvider::Huggingface => "huggingface",
        _ => "unknown",
    };
    let global_key = stored_keys.get(provider_name).cloned();
    drop(stored_keys); // Release lock before mutating config

    let api_key = config.api_key.clone().or(global_key);
    if let Some(key) = api_key {
        config.env_vars.insert(key_var.to_string(), key);
    }

    // Determine which runtime to use
    // Priority: per-agent runtime > global config runtime
    let agent_runtime = runtime.or_else(|| match state.config.container_runtime {
        crate::config::ContainerRuntimeType::Docker => Some("docker".to_string()),
        crate::config::ContainerRuntimeType::Exo => Some("exo".to_string()),
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
        gateway_port,
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

/// Execute a command in an agent container (e.g., open a shell)
pub async fn exec_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, (StatusCode, String)> {
    // Check if agent exists
    let (agent_exists, agent_runtime, agent_name) = {
        let containers = state.containers.read().await;
        containers
            .iter()
            .find(|a| a.id == id)
            .map(|a| (true, a.runtime.clone(), a.name.clone()))
            .unwrap_or((false, None, String::new()))
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

    // Build command - default to /bin/bash or /bin/sh
    let cmd = if let Some(command) = req.command {
        command
    } else {
        vec!["/bin/bash".to_string()]
    };

    // Execute command in container
    match runtime.exec_container(&id, cmd).await {
        Ok(output) => Ok(Json(ExecResponse {
            output,
            container_id: id,
            container_name: agent_name,
        })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Exec failed: {}", e))),
    }
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

        // Inject API key from agent config, or from global stored keys
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

        // First check agent-specific key, then fall back to globally stored key
        let stored_keys = state.api_keys.read().await;
        let provider_name = match agent.config.llm_provider {
            LlmProvider::Zai => "zai",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::OpenAI => "openai",
            LlmProvider::Kimi => "kimi",
            LlmProvider::KimiCode => "kimi-code",
            LlmProvider::Gemini => "google",
            LlmProvider::Access => "access",
            LlmProvider::Huggingface => "huggingface",
            _ => "unknown",
        };
        let global_key = stored_keys.get(provider_name).cloned();
        drop(stored_keys);

        let api_key = agent.config.api_key.clone().or(global_key);
        if let Some(key) = api_key {
            agent.config.env_vars.insert(key_var.to_string(), key);
        }

        // Ensure the gateway port is set in env vars
        agent
            .config
            .env_vars
            .insert("PORT".to_string(), agent.gateway_port.to_string());

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
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem = sys.total_memory();
    let available_mem = sys.available_memory();

    (total_mem, available_mem)
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
    Json(mut agent): Json<AgentContainer>,
) -> Result<Json<AgentContainer>, (StatusCode, String)> {
    // Choose runtime based on imported agent's runtime setting
    let runtime: &dyn ContainerRuntime = if agent.runtime.as_deref() == Some("exo") {
        &state.exo_runtime
    } else {
        &state.runtime
    };

    // Ensure gateway port is set in env vars
    agent
        .config
        .env_vars
        .insert("PORT".to_string(), agent.gateway_port.to_string());

    // Create the container
    let id = runtime
        .create_container(&agent.name, &agent.config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
/// Example: `ws://localhost:8081/api/agents/{id}/chat?token=eyJhbGciOiJIUzI1NiIs...`
pub async fn chat_websocket(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    // Log the connection attempt
    tracing::info!("WebSocket connection request for agent: {}", id);

    // Validate JWT token from query parameter
    tracing::info!("Validating token...");
    let token = params.get("token").ok_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication token".to_string(),
    ))?;

    let auth = state.auth.read().await;
    let _claims = auth
        .validate_token(token)
        .map_err(|e| {
            tracing::warn!("Token validation failed: {}", e);
            (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e))
        })?;
    drop(auth);
    tracing::info!("Token validated successfully");

    // Check if agent exists and is running - use index for O(1) lookup (critical for scalability)
    tracing::info!("Looking up agent in index...");
    let index = state.agent_index.read().await;
    let containers = state.containers.read().await;

    let agent = if let Some(&idx) = index.get(&id) {
        tracing::info!("Found agent at index {}", idx);
        containers.get(idx).ok_or_else(|| {
            tracing::error!("Agent not found in containers at index {}", idx);
            (StatusCode::NOT_FOUND, "Agent not found".to_string())
        })?
    } else {
        tracing::warn!("Agent ID not found in index");
        return Err((StatusCode::NOT_FOUND, "Agent not found".to_string()));
    };

    tracing::info!("Agent found: {} (status: {:?})", agent.name, agent.status);

    if agent.status != AgentStatus::Running {
        tracing::warn!("Agent {} is not running: {:?}", id, agent.status);
        return Err((StatusCode::BAD_REQUEST, "Agent is not running".to_string()));
    }

    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    let gateway_port = agent.gateway_port;
    drop(containers);
    drop(index);

    tracing::info!("Upgrading WebSocket connection for agent '{}' (ID: {}, port: {})", agent_name, id, gateway_port);
    tracing::info!("Calling on_upgrade...");

    let response = ws.on_upgrade(move |socket| handle_chat_stream(socket, state, agent_id, gateway_port));
    tracing::info!("on_upgrade called, returning response");

    Ok(response)
}

/// Get the IP address of a container from Docker with caching for scalability
async fn get_container_ip(state: &Arc<AppState>, container_id: &str) -> anyhow::Result<String> {
    // Check cache first - O(1) lookup (critical for scalability with thousands of agents)
    {
        let cache = state.container_ips.read().await;
        if let Some(ip) = cache.get(container_id) {
            return Ok(ip.clone());
        }
    }

    // Cache miss - fetch from Docker and cache it
    use bollard::container::InspectContainerOptions;
    use bollard::Docker;
    use bollard::API_DEFAULT_VERSION;

    // Create a new Docker connection (try Windows named pipe first, then Unix socket)
    let docker = if cfg!(windows) {
        Docker::connect_with_named_pipe(r"\\.\pipe\docker_engine", 120, API_DEFAULT_VERSION)
    } else {
        Docker::connect_with_socket("/var/run/docker.sock", 120, API_DEFAULT_VERSION)
    }
    .map_err(|e| anyhow::anyhow!("Failed to connect to Docker: {}", e))?;

    let inspect = docker
        .inspect_container(container_id, None::<InspectContainerOptions>)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to inspect container: {}", e))?;

    // Get the IP from the bridge network
    if let Some(networks) = inspect.network_settings.and_then(|n| n.networks) {
        for (_name, network) in networks {
            if let Some(ip) = network.ip_address {
                let ip_string = ip.to_string();

                // Cache the IP for future requests (avoids repeated Docker inspect calls)
                let mut cache = state.container_ips.write().await;
                cache.insert(container_id.to_string(), ip_string.clone());

                return Ok(ip_string);
            }
        }
    }

    Err(anyhow::anyhow!("No IP address found for container"))
}

async fn handle_chat_stream(socket: WebSocket, state: Arc<AppState>, agent_id: String, gateway_port: u16) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async_with_config, tungstenite::protocol::WebSocketConfig, tungstenite::Message as TungsteniteMessage};
    use std::time::Duration;

    println!("[handle_chat_stream] Starting for agent {} on port {}", agent_id, gateway_port);
    let (mut client_tx, mut client_rx) = socket.split();

    // Use localhost for orchestrator-to-agent connections (via Docker port forwarding)
    // Tailscale IPs are preserved for agent-to-agent communication
    let agent_ip = {
        let containers = state.containers.read().await;
        if let Some(agent) = containers.iter().find(|a| a.id == agent_id) {
            // Always use localhost for orchestrator-to-agent connections
            // The agent's gateway port is forwarded to localhost by Docker
            #[cfg(windows)]
            let ip = "127.0.0.1".to_string();
            #[cfg(not(windows))]
            let ip = match get_container_ip(&state, &agent_id).await {
                Ok(container_ip) => container_ip,
                Err(e) => {
                    tracing::warn!("Failed to get container IP for {}: {}", agent_id, e);
                    "127.0.0.1".to_string()
                }
            };
            println!("[handle_chat_stream] Using local IP {} for orchestrator-to-agent connection", ip);
            println!("[handle_chat_stream] Agent Tailscale IP {} preserved for agent-to-agent communication",
                agent.tailscale_ip.as_deref().unwrap_or("none"));
            ip
        } else {
            tracing::error!("Agent {} not found", agent_id);
            let error_msg = serde_json::json!({
                "role": "system",
                "error": "Agent not found",
                "content": format!("Agent {} not found in orchestrator", agent_id),
                "timestamp": chrono::Utc::now().timestamp()
            });
            let _ = client_tx.send(Message::Text(error_msg.to_string())).await;
            return;
        }
    };

    // Connect to agent websocket using OpenClaw protocol
    // OpenClaw gateway listens on the root path (no /ws or /chat path)
    let agent_ws_url = format!("ws://{}:{}", agent_ip, gateway_port);
    println!("[handle_chat_stream] Connecting to agent at: {}", agent_ws_url);

    // Configure websocket with more lenient settings
    let config = WebSocketConfig {
        max_send_queue: Some(100),
        accept_unmasked_frames: true,  // Be lenient with protocol
        ..Default::default()
    };

    let (agent_ws_stream, _) = match connect_async_with_config(&agent_ws_url, Some(config), false).await {
        Ok(result) => {
            println!("[handle_chat_stream] Successfully connected to agent!");
            tracing::info!("Connected to agent websocket at {}", agent_ws_url);
            result
        }
        Err(e) => {
            println!("[handle_chat_stream] ERROR connecting to agent: {:?}", e);
            tracing::warn!("Failed to connect to agent websocket at {}: {}", agent_ws_url, e);
            let error_msg = serde_json::json!({
                "role": "system",
                "error": "Could not connect to agent",
                "content": format!("Agent websocket unavailable: {}", e),
                "timestamp": chrono::Utc::now().timestamp()
            });
            let _ = client_tx.send(Message::Text(error_msg.to_string())).await;
            return;
        }
    };

    let (mut agent_tx, mut agent_rx) = agent_ws_stream.split();

    // Handle authentication challenge (password mode uses challenge-response)
    let auth_timeout = tokio::time::sleep(tokio::time::Duration::from_secs(5));
    tokio::pin!(auth_timeout);
    let mut authenticated = false;

    tracing::info!("Waiting for authentication challenge...");
    while !authenticated {
        tokio::select! {
            _ = &mut auth_timeout => {
                tracing::warn!("Timeout waiting for authentication challenge");
                let error_msg = serde_json::json!({
                    "role": "system",
                    "error": "Authentication timeout",
                    "content": "Agent did not send authentication challenge",
                    "timestamp": chrono::Utc::now().timestamp()
                });
                let _ = client_tx.send(Message::Text(error_msg.to_string())).await;
                return;
            }
            msg_result = agent_rx.next() => {
                match msg_result {
                    Some(Ok(TungsteniteMessage::Text(text))) => {
                        // Check if this is a challenge event
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if json["type"] == "event" && json["event"] == "connect.challenge" {
                                tracing::info!("Received authentication challenge, sending OpenClaw connect request...");
                                // Send OpenClaw connect request (RPC protocol)
                                // Note: OpenClaw uses type="req"/"res"/"event", method names, and params
                                let connect_request = serde_json::json!({
                                    "type": "req",
                                    "id": uuid::Uuid::new_v4().to_string(),
                                    "method": "connect",
                                    "params": {
                                        "minProtocol": 3,
                                        "maxProtocol": 3,
                                        "client": {
                                            "id": "cli",
                                            "version": env!("CARGO_PKG_VERSION"),
                                            "platform": "rust",
                                            "mode": "cli"
                                        },
                                        "role": "operator",
                                        "scopes": ["operator.read", "operator.write"],
                                        "caps": [],
                                        "commands": [],
                                        "permissions": {},
                                        "auth": {
                                            "password": "clawpen"
                                        },
                                        "locale": "en-US",
                                        "userAgent": format!("claw-pen-orchestrator/{}", env!("CARGO_PKG_VERSION"))
                                    }
                                });
                                tracing::info!("Sending connect request: {}", connect_request.to_string());

                                // Try to send the connect request
                                if agent_tx.send(TungsteniteMessage::Text(connect_request.to_string())).await.is_err() {
                                    tracing::error!("Failed to send connect request");
                                    return;
                                }

                                tracing::info!("Connect request sent successfully, waiting for response...");
                                // Wait for agent's response to our connect request
                                let confirm_timeout = tokio::time::sleep(tokio::time::Duration::from_secs(3));
                                tokio::pin!(confirm_timeout);
                                let mut auth_confirmed = false;

                                // Read next message to see if auth succeeded
                                loop {
                                    tokio::select! {
                                        _ = &mut confirm_timeout => {
                                            tracing::warn!("Timeout waiting for authentication response");
                                            break;
                                        }
                                        confirm_result = agent_rx.next() => {
                                            match confirm_result {
                                                Some(Ok(TungsteniteMessage::Text(confirm_text))) => {
                                                    tracing::info!("Received response: {}", confirm_text);
                                                    if let Ok(confirm_json) = serde_json::from_str::<serde_json::Value>(&confirm_text) {
                                                        // OpenClaw sends type="res" responses
                                                        if confirm_json["type"] == "res" {
                                                            if confirm_json["ok"] == true {
                                                                tracing::info!("✅ Authentication successful! Connected to OpenClaw agent");
                                                                auth_confirmed = true;
                                                                break;
                                                            } else {
                                                                tracing::error!("❌ Authentication failed: {}", confirm_text);
                                                                let error_msg = confirm_json["error"].as_str().unwrap_or("Unknown error");
                                                                tracing::error!("Error details: {}", error_msg);
                                                                return;
                                                            }
                                                        }
                                                    }
                                                    // If not an error, assume success and continue
                                                    auth_confirmed = true;
                                                    break;
                                                }
                                                Some(Ok(TungsteniteMessage::Close(_))) => {
                                                    tracing::error!("Agent closed connection after authentication attempt");
                                                    return;
                                                }
                                                Some(Err(e)) => {
                                                    tracing::error!("WebSocket error after auth: {}", e);
                                                    return;
                                                }
                                                None => {
                                                    tracing::warn!("Agent stream ended after auth");
                                                    return;
                                                }
                                                Some(Ok(_)) => {}
                                            }
                                        }
                                    }
                                }

                                if auth_confirmed {
                                    authenticated = true;
                                } else {
                                    tracing::warn!("Authentication not explicitly confirmed, proceeding anyway");
                                    authenticated = true;
                                }
                                continue;
                            }
                        }
                        // Forward any other messages to client (might be auth confirmation or other events)
                        if client_tx.send(Message::Text(text)).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(TungsteniteMessage::Close(_))) => {
                        tracing::warn!("Agent closed connection during authentication");
                        return;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error during authentication: {}", e);
                        return;
                    }
                    None => {
                        tracing::warn!("Agent stream ended during authentication");
                        return;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    tracing::info!("Connected to agent gateway, starting message proxy");

    // Send immediate acknowledgment to browser to complete WebSocket handshake
    let connect_ack = serde_json::json!({
        "role": "system",
        "content": "Connected to agent",
        "type": "event",
        "event": "connection.established",
        "timestamp": chrono::Utc::now().timestamp()
    });
    if let Err(e) = client_tx.send(Message::Text(connect_ack.to_string())).await {
        tracing::error!("Failed to send connection acknowledgment: {}", e);
        return;
    }
    tracing::info!("Sent connection acknowledgment to browser");

    // Spawn a task to forward messages from agent to client
    let agent_to_client = tokio::spawn(async move {
        while let Some(msg_result) = agent_rx.next().await {
            match msg_result {
                Ok(TungsteniteMessage::Text(text)) => {
                    if client_tx.send(Message::Text(text)).await.is_err() {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Close(_)) => {
                    break;
                }
                Err(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    // Forward messages from client to agent with OpenClaw protocol translation
    let mut request_id_counter = 0u64;
    while let Some(msg_result) = client_rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                tracing::info!("Received from GUI: {}", text);

                // Parse the client message
                if let Ok(mut client_msg) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Check if this is already in OpenClaw format (has "method" field)
                    if let Some(method) = client_msg.get("method").and_then(|m| m.as_str()) {
                        // Check if it's a chat.send message that needs fixing
                        if method == "chat.send" {
                            // Fix sessionKey if it's in the short format
                            if let Some(params) = client_msg.get_mut("params") {
                                if let Some(session_key) = params.get("sessionKey").and_then(|k| k.as_str()) {
                                    if session_key == "main" || session_key == "dev" {
                                        params["sessionKey"] = serde_json::json!("agent:dev:main");
                                    }
                                }

                                // Add idempotencyKey if missing
                                if params.get("idempotencyKey").is_none() {
                                    request_id_counter += 1;
                                    let idempotency_key = format!("idem-{}-{}", chrono::Utc::now().timestamp(), request_id_counter);
                                    params["idempotencyKey"] = serde_json::json!(idempotency_key);
                                }

                                // Remove "deliver" field if present (not an OpenClaw param)
                                if let Some(_deliver) = params.get("deliver") {
                                    let mut params_obj = params.as_object().unwrap().clone();
                                    params_obj.remove("deliver");
                                    client_msg["params"] = serde_json::json!(params_obj);
                                }
                            }

                            let fixed_msg = client_msg.to_string();
                            tracing::info!("Fixed chat.send message: {}", fixed_msg);
                            if agent_tx.send(TungsteniteMessage::Text(fixed_msg)).await.is_err() {
                                break;
                            }
                        } else {
                            // Other OpenClaw methods - forward as-is
                            tracing::debug!("Forwarding OpenClaw message: {}", text);
                            if agent_tx.send(TungsteniteMessage::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    } else {
                        // Check if this is a client format message with "content"
                        let content = client_msg.get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        // Only translate messages that have actual content to send
                        if !content.is_empty() {
                            let session = client_msg.get("session")
                                .and_then(|v| v.as_str())
                                .unwrap_or("main");

                            // Generate unique IDs
                            request_id_counter += 1;
                            let req_id = format!("req-{}", request_id_counter);
                            let idempotency_key = format!("idem-{}-{}", chrono::Utc::now().timestamp(), request_id_counter);

                            // Translate to OpenClaw chat.send format
                            let openclaw_msg = serde_json::json!({
                                "type": "req",
                                "id": req_id,
                                "method": "chat.send",
                                "params": {
                                    "sessionKey": format!("agent:dev:{}", session),
                                    "message": content,
                                    "idempotencyKey": idempotency_key
                                }
                            });

                            tracing::info!("Translated client message to OpenClaw format: {}", openclaw_msg.to_string());

                            if agent_tx.send(TungsteniteMessage::Text(openclaw_msg.to_string())).await.is_err() {
                                break;
                            }
                        } else {
                            // Skip control messages or empty messages
                            let msg_type = client_msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            tracing::debug!("Skipping non-chat message: type='{}', content='{}'", msg_type, content);
                        }
                    }
                } else {
                    // If parsing fails, forward as-is (might already be OpenClaw format)
                    tracing::warn!("Failed to parse client message, forwarding as-is: {}", text);
                    if agent_tx.send(TungsteniteMessage::Text(text)).await.is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                let _ = agent_tx.send(TungsteniteMessage::Close(None)).await;
                break;
            }
            Err(_) => {
                break;
            }
            _ => {}
        }
    }

    // Clean up
    agent_to_client.abort();
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

// === Volume Management ===

pub async fn list_volumes(State(state): State<Arc<AppState>>) -> Json<Vec<Volume>> {
    let volumes = state.volumes.read().await;
    Json(volumes.clone())
}

pub async fn get_volume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Volume>, (StatusCode, String)> {
    let volumes = state.volumes.read().await;
    volumes
        .iter()
        .find(|v| v.id == id)
        .map(|v| Json(v.clone()))
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Volume not found".to_string()))
}

pub async fn create_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<Json<Volume>, (StatusCode, String)> {
    // Validate name
    if let Err(e) = validation::validate_container_name(&req.name) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid volume name: {}", e),
        ));
    }

    let mut volumes = state.volumes.write().await;

    // Check for duplicate name
    if volumes.iter().any(|v| v.name == req.name) {
        return Err((
            StatusCode::CONFLICT,
            "Volume with this name already exists".to_string(),
        ));
    }

    // Generate ID
    let id = format!("vol-{}", uuid::Uuid::new_v4());

    // Determine host path
    let host_path = if let Some(ref path) = req.host_path {
        // Validate the path exists
        if !std::path::Path::new(path).exists() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Host path does not exist: {}", path),
            ));
        }
        Some(path.clone())
    } else {
        // Create managed volume directory
        let vol_dir = state.data_dir.join("volumes").join(&id);
        if let Err(e) = std::fs::create_dir_all(&vol_dir) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create volume directory: {}", e),
            ));
        }
        Some(vol_dir.to_string_lossy().to_string())
    };

    let volume = Volume {
        id: id.clone(),
        name: req.name,
        description: req.description,
        host_path,
        default_target: req.default_target,
        read_only: req.read_only,
        size_mb: req.size_mb,
        tags: req.tags,
        created_at: chrono::Utc::now().to_rfc3339(),
        attached_agents: vec![],
    };

    volumes.push(volume.clone());
    crate::save_volumes(&state.data_dir, &volumes);

    tracing::info!("Created volume {} ({})", volume.name, volume.id);
    Ok(Json(volume))
}

pub async fn update_volume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateVolumeRequest>,
) -> Result<Json<Volume>, (StatusCode, String)> {
    let mut volumes = state.volumes.write().await;

    // Check for duplicate name first if name is being updated
    if let Some(ref name) = req.name {
        if volumes.iter().any(|v| v.id != id && v.name == *name) {
            return Err((
                StatusCode::CONFLICT,
                "Volume with this name already exists".to_string(),
            ));
        }
        if let Err(e) = validation::validate_container_name(name) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid volume name: {}", e),
            ));
        }
    }

    let volume = volumes
        .iter_mut()
        .find(|v| v.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Volume not found".to_string()))?;

    if let Some(name) = req.name {
        volume.name = name;
    }
    if let Some(desc) = req.description {
        volume.description = Some(desc);
    }
    if let Some(target) = req.default_target {
        volume.default_target = target;
    }
    if let Some(ro) = req.read_only {
        volume.read_only = ro;
    }
    if let Some(tags) = req.tags {
        volume.tags = tags;
    }

    let updated = volume.clone();
    crate::save_volumes(&state.data_dir, &volumes);

    Ok(Json(updated))
}

pub async fn delete_volume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut volumes = state.volumes.write().await;

    let volume = volumes
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Volume not found".to_string()))?;

    // Check if volume is attached to any agents
    if !volume.attached_agents.is_empty() {
        return Err((
            StatusCode::CONFLICT,
            format!("Volume is attached to agents: {:?}", volume.attached_agents),
        ));
    }

    let idx = volumes.iter().position(|v| v.id == id).unwrap();
    let removed = volumes.remove(idx);

    crate::save_volumes(&state.data_dir, &volumes);

    // Optionally delete managed volume directory
    if let Some(ref path) = removed.host_path {
        if path.starts_with(&state.data_dir.to_string_lossy().to_string()) {
            // This is a managed volume, delete the directory
            if let Err(e) = std::fs::remove_dir_all(path) {
                tracing::warn!("Failed to delete volume directory: {}", e);
            }
        }
    }

    tracing::info!("Deleted volume {}", id);
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// SERVICE DISCOVERY & TAILSCALE IP MANAGEMENT
// ============================================================================

/// Get an agent's Tailscale IP address
pub async fn get_agent_tailscale_ip(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<String>>, (StatusCode, String)> {
    let containers = state.containers.read().await;

    let agent = containers
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    Ok(Json(agent.tailscale_ip.clone()))
}

/// Update an agent's Tailscale IP address
/// Called automatically when an agent connects to Tailscale
pub async fn update_agent_tailscale_ip(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(ip): Json<String>,
) -> Result<Json<Option<String>>, (StatusCode, String)> {
    let mut containers = state.containers.write().await;

    let agent = containers
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    agent.tailscale_ip = Some(ip.clone());

    // Convert to StoredAgent format and persist
    let status = format!("{:?}", agent.status);
    let config = agent.config.clone();
    let runtime = agent.runtime.clone();
    let gateway_port = agent.gateway_port;

    // Need to drop the mutable borrow before creating the stored_agents vector
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    let agent_tailscale_ip = agent.tailscale_ip.clone();

    // Now convert all containers to StoredAgent format
    let stored_agents: Vec<storage::StoredAgent> = containers
        .iter()
        .map(|c| storage::StoredAgent {
            id: c.id.clone(),
            name: c.name.clone(),
            status: format!("{:?}", c.status),
            config: c.config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            runtime: c.runtime.clone(),
            gateway_port: Some(c.gateway_port),
            tailscale_ip: c.tailscale_ip.clone(),
        })
        .collect();

    if let Err(e) = storage::save_agents(&stored_agents) {
        tracing::error!("Failed to save agents: {}", e);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save agent data: {}", e)));
    }

    tracing::info!("Updated agent {} Tailscale IP to {}", agent_id, ip);
    Ok(Json(agent_tailscale_ip))
}

/// List all agents with their Tailscale IPs (for service discovery)
pub async fn list_agents_with_tailscale(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TailscaleQueryParams>,
) -> Result<Json<Vec<AgentContainer>>, (StatusCode, String)> {
    let containers = state.containers.read().await;

    let agents: Vec<AgentContainer> = if params.only_with_tailscale.unwrap_or(false) {
        containers
            .iter()
            .filter(|a| a.tailscale_ip.is_some())
            .cloned()
            .collect()
    } else {
        containers.clone()
    };

    Ok(Json(agents))
}

/// Query parameters for Tailscale agent listing
#[derive(Debug, serde::Deserialize)]
pub struct TailscaleQueryParams {
    /// Only return agents that have Tailscale IPs
    only_with_tailscale: Option<bool>,
}

/// Extract Tailscale IP from a running container's logs
pub async fn extract_tailscale_ip_from_container(
    state: &Arc<AppState>,
    container_name: &str,
) -> anyhow::Result<Option<String>> {
    use crate::container::ContainerRuntime;

    // Get container logs and search for Tailscale IP
    let logs = state.runtime.container_logs(container_name, 100).await?;

    // Look for the Tailscale connection line
    for line in logs.lines() {
        if line.contains("=== Tailscale connected ===") {
            // Next line should have the IP
            continue;
        }
        if line.chars().all(|c| c.is_numeric() || c == '.') {
            // This looks like an IP address
            if line.starts_with("100.") || line.starts_with("fd") {
                // Tailscale IPs start with 100. or fd
                return Ok(Some(line.trim().to_string()));
            }
        }
    }

    Ok(None)
}

/// Automatically discover and register Tailscale IPs for all running agents
/// This should be called periodically or triggered by container start events
pub async fn discover_tailscale_ips(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DiscoveredAgents>, (StatusCode, String)> {
    let containers = state.containers.read().await;
    let mut discovered = Vec::new();

    // Collect agents that need Tailscale IP discovery
    let agents_to_update: Vec<_> = containers
        .iter()
        .filter(|a| a.status == AgentStatus::Running && a.tailscale_ip.is_none())
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();

    // Release the lock before doing expensive operations
    drop(containers);

    // Process each agent
    for (agent_id, agent_name) in agents_to_update {
        // Try to extract Tailscale IP from the container
        if let Ok(Some(ip)) = extract_tailscale_ip_from_container(&state, &agent_name).await {
            tracing::info!("Discovered Tailscale IP {} for agent {}", ip, agent_name);

            // Use the update function to persist the IP
            let _ = update_agent_tailscale_ip(
                Path(agent_id.clone()),
                State(state.clone()),
                Json(ip.clone()),
            ).await;

            discovered.push(DiscoveredAgent {
                agent_id,
                agent_name,
                tailscale_ip: ip,
            });
        }
    }

    Ok(Json(DiscoveredAgents { agents: discovered }))
}

/// Response format for Tailscale IP discovery
#[derive(Debug, serde::Serialize)]
pub struct DiscoveredAgents {
    pub agents: Vec<DiscoveredAgent>,
}

/// A discovered agent with its Tailscale IP
#[derive(Debug, serde::Serialize)]
pub struct DiscoveredAgent {
    pub agent_id: String,
    pub agent_name: String,
    pub tailscale_ip: String,
}

/// Trigger Tailscale IP discovery for all running agents
pub async fn trigger_discovery(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DiscoveredAgents>, (StatusCode, String)> {
    discover_tailscale_ips(State(state.clone())).await
}

/// Get service registry - all agents that can communicate via Tailscale
pub async fn get_service_registry(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServiceRegistry>, (StatusCode, String)> {
    let containers = state.containers.read().await;

    let services: Vec<AgentService> = containers
        .iter()
        .filter(|a| a.status == AgentStatus::Running && a.tailscale_ip.is_some())
        .map(|agent| {
            let ip = agent.tailscale_ip.as_ref().unwrap();
            AgentService {
                id: agent.id.clone(),
                name: agent.name.clone(),
                tailscale_ip: ip.clone(),
                gateway_url: format!("ws://{}:{}", ip, agent.gateway_port),
                status: format!("{:?}", agent.status),
                capabilities: vec!["chat".to_string(), "rpc".to_string(), "workflow".to_string()], // TODO: Make this dynamic
            }
        })
        .collect();

    Ok(Json(ServiceRegistry { agents: services }))
}

/// Service registry response
#[derive(Debug, serde::Serialize)]
pub struct ServiceRegistry {
    pub agents: Vec<AgentService>,
}

/// An agent in the service registry
#[derive(Debug, serde::Serialize)]
pub struct AgentService {
    pub id: String,
    pub name: String,
    pub tailscale_ip: String,
    pub gateway_url: String,
    pub status: String,
    pub capabilities: Vec<String>,
}

// ============================================================================
// AGENT-TO-AGENT MESSAGE ROUTING
// ============================================================================

/// Send a message from one agent to another
pub async fn send_message(
    Path(from_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, (StatusCode, String)> {
    use crate::types::{AgentMessage, DirectMessage, MessageStatus, RequestMessage};
    use uuid::Uuid;

    // Verify sender agent exists
    let containers = state.containers.read().await;
    let sender = containers
        .iter()
        .find(|a| a.id == from_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Sender agent not found".to_string()))?;

    if sender.status != crate::types::AgentStatus::Running {
        return Err((StatusCode::BAD_REQUEST, "Sender agent is not running".to_string()));
    }

    // Find recipient agent (by ID or name)
    let recipient = containers
        .iter()
        .find(|a| a.id == request.to || a.name == request.to)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Recipient agent not found".to_string()))?;

    // Check if recipient has a Tailscale IP
    let recipient_ip = recipient.tailscale_ip.as_ref()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Recipient agent is not on Tailscale network".to_string()))?;

    // Generate message ID
    let message_id = Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Create the message based on type
    let message = match request.message_type.as_str() {
        "request" => AgentMessage::Request(RequestMessage {
            id: message_id.clone(),
            from: from_id.clone(),
            to: recipient.id.clone(),
            content: request.content.clone(),
            timestamp: timestamp.clone(),
            timeout: request.timeout.unwrap_or(30),
            metadata: request.metadata.clone(),
        }),
        _ => AgentMessage::Direct(DirectMessage {
            id: message_id.clone(),
            from: from_id.clone(),
            to: recipient.id.clone(),
            content: request.content.clone(),
            timestamp: timestamp.clone(),
            metadata: request.metadata.clone(),
        }),
    };

    // Serialize the message
    let message_json = serde_json::to_string(&message)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize message: {}", e)))?;

    // Send the message to the recipient via Tailscale
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/api/message", recipient_ip, recipient.gateway_port);

    let send_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        client.post(&url)
            .header("Content-Type", "application/json")
            .body(message_json.clone())
            .send()
    ).await;

    match send_result {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                tracing::info!("Message {} delivered from {} to {}", message_id, from_id, recipient.id);
                Ok(Json(SendMessageResponse {
                    message_id: message_id.clone(),
                    status: MessageStatus::Delivered,
                    response: None,
                    error: None,
                }))
            } else {
                let error_text = response.text().await.unwrap_or_default();
                tracing::warn!("Failed to deliver message {}: {}", message_id, error_text);
                Ok(Json(SendMessageResponse {
                    message_id: message_id.clone(),
                    status: MessageStatus::Failed,
                    response: None,
                    error: Some(format!("Recipient rejected message: {}", error_text)),
                }))
            }
        }
        Ok(Err(e)) => {
            tracing::error!("Network error sending message {}: {}", message_id, e);
            Ok(Json(SendMessageResponse {
                message_id: message_id.clone(),
                status: MessageStatus::Failed,
                response: None,
                error: Some(format!("Network error: {}", e)),
            }))
        }
        Err(_) => {
            tracing::error!("Timeout sending message {}", message_id);
            Ok(Json(SendMessageResponse {
                message_id: message_id.clone(),
                status: MessageStatus::Failed,
                response: None,
                error: Some("Request timeout".to_string()),
            }))
        }
    }
}

/// Get messages for an agent
pub async fn get_agent_messages(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TrackedMessage>>, (StatusCode, String)> {
    // Verify agent exists
    let containers = state.containers.read().await;
    let _agent = containers
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // TODO: Implement message persistence and retrieval
    // For now, return empty list
    Ok(Json(vec![]))
}

/// Tracked message wrapper for API responses
#[derive(Debug, serde::Serialize)]
pub struct TrackedMessage {
    pub id: String,
    pub from: String,
    pub to: Option<String>,
    pub message_type: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub delivered_at: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// WEBSOCKET PROXY FOR AGENT-TO-AGENT COMMUNICATION
// ============================================================================

/// WebSocket proxy for agent-to-agent communication
/// Route: GET /api/agents/:id/ws/:target_id
/// This endpoint upgrades the connection to WebSocket and proxies to the target agent
pub async fn websocket_proxy(
    Path((from_id, target_id)): Path<(String, String)>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket_proxy(socket, from_id, target_id, state))
}

/// Handle the WebSocket proxy connection
async fn handle_websocket_proxy(
    client_socket: WebSocket,
    from_id: String,
    target_id: String,
    state: Arc<AppState>,
) {
    use axum::extract::ws::Message as AxumMessage;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    // Verify both agents exist and are running
    let containers = state.containers.read().await;

    let from_agent = match containers.iter().find(|a| a.id == from_id) {
        Some(agent) => agent,
        None => {
            tracing::error!("Source agent {} not found", from_id);
            return;
        }
    };

    let target_agent = match containers.iter().find(|a| a.id == target_id || a.name == target_id) {
        Some(agent) => agent,
        None => {
            tracing::error!("Target agent {} not found", target_id);
            return;
        }
    };

    // Get target agent's Tailscale IP and gateway port
    let target_ip = match target_agent.tailscale_ip.as_ref() {
        Some(ip) => ip,
        None => {
            tracing::error!("Target agent {} is not on Tailscale network", target_id);
            return;
        }
    };

    let target_url = format!("ws://{}:{}/gateway", target_ip, target_agent.gateway_port);
    tracing::info!("Proxying WebSocket: {} -> {} ({})", from_agent.name, target_agent.name, target_url);

    // Drop the read lock before connecting
    drop(containers);

    // Connect to target agent's WebSocket gateway
    let target_ws = match tokio_tungstenite::connect_async(&target_url).await {
        Ok((socket, _)) => socket,
        Err(e) => {
            tracing::error!("Failed to connect to target agent {}: {}", target_id, e);
            return;
        }
    };

    let (mut client_sender, mut client_receiver) = client_socket.split();
    let (mut target_sender, mut target_receiver) = target_ws.split();

    // Spawn task to forward messages from client to target
    let client_to_target = tokio::spawn(async move {
        while let Some(result) = client_receiver.next().await {
            match result {
                Ok(AxumMessage::Text(text)) => {
                    tracing::debug!("Client -> Target: {}", text);
                    if let Err(e) = target_sender.send(TungsteniteMessage::Text(text)).await {
                        tracing::error!("Failed to send to target: {}", e);
                        break;
                    }
                }
                Ok(AxumMessage::Close(_)) => {
                    tracing::info!("Client closed connection");
                    let _ = target_sender.send(TungsteniteMessage::Close(None)).await;
                    break;
                }
                Err(e) => {
                    tracing::error!("Error receiving from client: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Spawn task to forward messages from target to client
    let target_to_client = tokio::spawn(async move {
        while let Some(result) = target_receiver.next().await {
            match result {
                Ok(TungsteniteMessage::Text(text)) => {
                    tracing::debug!("Target -> Client: {}", text);
                    if let Err(e) = client_sender.send(AxumMessage::Text(text)).await {
                        tracing::error!("Failed to send to client: {}", e);
                        break;
                    }
                }
                Ok(TungsteniteMessage::Close(_)) => {
                    tracing::info!("Target closed connection");
                    let _ = client_sender.send(AxumMessage::Close(None)).await;
                    break;
                }
                Err(e) => {
                    tracing::error!("Error receiving from target: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either direction to complete
    tokio::select! {
        _ = client_to_target => {
            tracing::info!("Client to target forwarding completed");
        }
        _ = target_to_client => {
            tracing::info!("Target to client forwarding completed");
        }
    }

    tracing::info!("WebSocket proxy session ended: {} -> {}", from_id, target_id);
}
