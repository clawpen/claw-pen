use std::collections::HashMap;
mod agent_comms;
mod andor;
mod chat_db;
mod direct_llm;
mod api;
mod auth;
mod config;
mod container;
mod containment;
mod inference;
mod network;
mod rpc;
mod secret_manager;
mod shared_memory;
mod snapshots;
mod storage;
mod teams;
mod templates;
mod types;
mod validation;
mod volume_attachment;
mod workflow;
mod executor;
// mod code_index;  // TODO: fix dependencies

use axum::http::{header, HeaderValue, Method};
use axum::{
    routing::{delete, get, post},
    Router,
};
use container::ContainerRuntime;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use crate::auth::AuthManager;
use crate::secret_manager::SecretsManager;
use crate::snapshots::SnapshotManager;

pub struct AppState {
    pub config: config::Config,
    pub containers: std::sync::Arc<RwLock<Vec<types::AgentContainer>>>,
    pub runtime: container::RuntimeClient,
    /// Exo-specific runtime for agents that use exo
    pub exo_runtime: container::RuntimeClient,
    pub templates: templates::TemplateRegistry,
    pub andor: Option<andor::AndorClient>,
    pub secrets: SecretsManager,
    pub snapshots: SnapshotManager,
    pub teams: teams::TeamRegistry,
    pub api_keys: RwLock<HashMap<String, String>>,
    pub data_dir: std::path::PathBuf,
    pub auth: RwLock<AuthManager>,
    /// Named volumes that can be attached to agents
    pub volumes: RwLock<Vec<types::Volume>>,
    /// Cache container IPs to avoid Docker inspect on every connection (critical for scalability)
    pub container_ips: RwLock<std::collections::HashMap<String, String>>,
    /// Agent index for O(1) lookups by ID (critical for scaling to thousands of agents)
    pub agent_index: RwLock<std::collections::HashMap<String, usize>>,
    /// RPC client for agent-to-agent communication
    pub rpc_client: rpc::RpcClient,
    /// Workflow registry for managing workflow definitions
    pub workflows: std::sync::Arc<tokio::sync::RwLock<workflow::WorkflowRegistry>>,
    /// Workflow executor for running workflows
    pub executor: std::sync::Arc<executor::WorkflowExecutor>,
    /// Native inference service manager (optional)
    pub inference: Option<inference::InferenceManager>,
    /// Chat database — users, classes, conversations, LTI mapping.
    /// Message transcripts stay in JSONL; this holds metadata + indexes.
    pub chat_db: std::sync::Arc<chat_db::ChatDb>,
}

fn load_api_keys(data_dir: &std::path::Path) -> HashMap<String, String> {
    let keys_path = data_dir.join("api_keys.json");
    if keys_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&keys_path) {
            if let Ok(keys) = serde_json::from_str(&contents) {
                return keys;
            }
        }
    }
    HashMap::new()
}

fn load_volumes(data_dir: &std::path::Path) -> Vec<types::Volume> {
    let volumes_path = data_dir.join("volumes.json");
    if volumes_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&volumes_path) {
            if let Ok(volumes) = serde_json::from_str(&contents) {
                return volumes;
            }
        }
    }
    Vec::new()
}

fn save_volumes(data_dir: &std::path::Path, volumes: &[types::Volume]) {
    let volumes_path = data_dir.join("volumes.json");
    if let Ok(contents) = serde_json::to_string_pretty(volumes) {
        if let Err(e) = std::fs::write(&volumes_path, contents) {
            tracing::error!("Failed to save volumes: {}", e);
        }
    }
}

/// Background task to sync container states with agent statuses
/// This ensures that agents that have crashed or stopped are marked correctly
async fn sync_container_states(state: &AppState) -> anyhow::Result<()> {
    use crate::types::AgentStatus;

    // Get actual container states from both runtimes (graceful if unavailable)
    let mut runtime_containers = state.runtime.list_containers().await.unwrap_or_else(|e| {
        tracing::debug!("Could not list containers during sync: {}", e);
        Vec::new()
    });
    if let Ok(exo_containers) = state.exo_runtime.list_containers().await {
        runtime_containers.extend(exo_containers);
    }

    // Map by container NAME since that's the common identifier
    let runtime_map_by_name: std::collections::HashMap<String, AgentStatus> = runtime_containers
        .iter()
        .map(|c| (c.name.clone(), c.status.clone()))
        .collect();

    let mut containers = state.containers.write().await;
    let mut has_changes = false;

    for agent in containers.iter_mut() {
        let actual_status = runtime_map_by_name.get(&agent.name);

        match actual_status {
            Some(AgentStatus::Stopped) | Some(AgentStatus::Error) => {
                // Container exists but runtime reports stopped/error.
                // For Exo, `ps` lies in rootless WSL — verify with a gateway TCP probe
                // before downgrading. If the gateway answers, the agent is alive.
                if agent.status == AgentStatus::Running {
                    let gateway_alive = if agent.runtime.as_deref() == Some("exo") {
                        let addr = format!("127.0.0.1:{}", agent.gateway_port);
                        tokio::time::timeout(
                            std::time::Duration::from_millis(300),
                            tokio::net::TcpStream::connect(&addr),
                        )
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .is_some()
                    } else {
                        false
                    };

                    if gateway_alive {
                        tracing::debug!(
                            "Agent {} runtime reports {:?} but gateway is reachable — keeping Running",
                            agent.name,
                            actual_status
                        );
                    } else {
                        tracing::warn!(
                            "Agent {} was marked as running but container is actually {:?}",
                            agent.name,
                            actual_status
                        );
                        agent.status = actual_status.map(|s| s.clone()).unwrap_or(AgentStatus::Error);
                        has_changes = true;
                    }
                }
            }
            Some(AgentStatus::Running) => {
                // Container is running - update if we thought it was stopped
                if agent.status != AgentStatus::Running {
                    tracing::info!(
                        "Agent {} container is running, updating status from {:?}",
                        agent.name,
                        agent.status
                    );
                    agent.status = AgentStatus::Running;
                    has_changes = true;
                }
            }
            Some(AgentStatus::Starting) | Some(AgentStatus::Stopping) => {
                // Container is in transition state - treat as running
                if agent.status != AgentStatus::Running && agent.status != AgentStatus::Starting {
                    agent.status = AgentStatus::Starting;
                    has_changes = true;
                }
            }
            None => {
                // Container doesn't exist in any runtime — but direct-runtime
                // agents have no container by design, so leave them alone.
                if agent.runtime.as_deref() == Some("direct") {
                    // direct agents are Running iff start_agent set them so;
                    // there's nothing for the reconciler to verify here.
                } else if agent.status == AgentStatus::Running || agent.status == AgentStatus::Starting {
                    tracing::warn!(
                        "Agent {} was marked as running but container not found",
                        agent.name
                    );
                    agent.status = AgentStatus::Stopped;
                    has_changes = true;
                }
            }
        }
    }

    // Rebuild agent index if there were changes
    if has_changes {
        let agent_index: std::collections::HashMap<String, usize> = containers
            .iter()
            .enumerate()
            .map(|(idx, agent)| (agent.id.clone(), idx))
            .collect();

        // Update the index
        let mut index = state.agent_index.write().await;
        *index = agent_index;
        drop(index);

        // Persist changes
        for agent in containers.iter() {
            if let Err(e) = crate::storage::upsert_agent(&crate::storage::to_stored_agent(agent)) {
                tracing::warn!("Failed to persist agent status for {}: {}", agent.name, e);
            }
        }
    }

    Ok(())
}

async fn health_check_all_agents(state: &AppState) -> anyhow::Result<()> {
    use crate::types::{AgentStatus, HealthStatus};

    let mut containers = state.containers.write().await;
    for agent in containers.iter_mut() {
        if agent.status != AgentStatus::Running {
            continue;
        }

        let gateway_reachable = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", agent.gateway_port)),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_some();

        let status = HealthStatus {
            healthy: gateway_reachable,
            last_check: chrono::Utc::now().to_rfc3339(),
            message: Some(if gateway_reachable {
                format!("Agent '{}' gateway reachable on port {}", agent.name, agent.gateway_port)
            } else {
                format!("Agent '{}' gateway not responding on port {}", agent.name, agent.gateway_port)
            }),
        };

        agent.health_status = Some(status);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Check for CLI password setting mode
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--set-password".to_string()) {
        let data_dir = std::path::PathBuf::from("./data");
        auth::cli_set_password(&data_dir)?;
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let config = config::load()?;
    // Use relative path for cross-platform compatibility
    let data_dir = std::path::PathBuf::from("./data");
    std::fs::create_dir_all(&data_dir).ok();
    tracing::info!("Loaded config: {:?}", config);

    // Initialize Auth Manager
    let auth_manager = AuthManager::new(&data_dir)?;
    if !auth_manager.has_admin() {
        tracing::warn!("⚠️  No admin password set. Use --set-password to set one, or enable ENABLE_REGISTRATION=true for first-time setup.");
    } else {
        tracing::info!("Authentication initialized - admin user configured");
    }

    // Load templates
    let template_registry = templates::TemplateRegistry::load()?;
    tracing::info!("Loaded {} templates", template_registry.list().len());

    // Initialize AndOR Bridge client if configured
    let andor_client = config.andor_bridge.as_ref().map(|cfg| {
        tracing::info!("AndOR Bridge configured at {}", cfg.url);
        andor::AndorClient::new(cfg.clone())
    });

    // Initialize native inference service if configured
    let inference_manager = if let Some(inf_config) = &config.native_inference {
        tracing::info!("Native inference configured with model: {}", inf_config.model_path);
        let manager = inference::InferenceManager::new(inf_config.clone());

        // Try to start the inference service
        match manager.start().await {
            Ok(()) => {
                tracing::info!("Native inference service started on port {}", inf_config.port);
                Some(manager)
            }
            Err(e) => {
                tracing::warn!("Failed to start native inference service: {}. The service will be unavailable.", e);
                None
            }
        }
    } else {
        tracing::info!("Native inference not configured");
        None
    };

    // Connect to primary runtime (based on global config)
    let runtime = container::RuntimeClient::with_runtime(
        config.container_runtime.clone(),
        config.exo_path.clone(),
    )
    .await?
    .with_network_config(
        config.network_backend,
        config.tailscale_auth_key.clone(),
        config.headscale_url.clone(),
        config.headscale_auth_key.clone(),
        config.headscale_namespace.clone(),
    );

    tracing::info!(
        "Connected to primary container runtime: {:?}",
        config.container_runtime
    );

    // Always initialize exo runtime as secondary (for per-agent selection)
    // This allows agents to use exo even if docker is the global default
    let exo_runtime = match container::RuntimeClient::with_runtime(
        config::ContainerRuntimeType::Exo,
        config.exo_path.clone(),
    )
    .await
    {
        Ok(client) => {
            tracing::info!("Exo runtime available for per-agent selection");
            client
        }
        Err(e) => {
            tracing::warn!(
                "Exo runtime not available (per-agent exo selection will fail): {}",
                e
            );
            // Fall back to primary runtime - operations will fail gracefully
            runtime.clone_runtime_client()
        }
    };

    // Load persisted agents from storage
    let stored_agents = storage::load_agents().unwrap_or_default();
    tracing::info!("Loaded {} persisted agents", stored_agents.len());

    // Get runtime containers to update status (graceful fallback if Docker is unavailable)
    let runtime_containers = runtime.list_containers().await.unwrap_or_else(|e| {
        tracing::warn!("Could not list containers (Docker may not be running): {}", e);
        Vec::new()
    });
    let runtime_ids: std::collections::HashSet<String> =
        runtime_containers.iter().map(|c| c.id.clone()).collect();

    // Merge persisted agents with runtime state
    let mut merged_agents = Vec::new();
    for stored in stored_agents {
        // Check if this agent is actually running in the runtime
        let status = if runtime_ids.contains(&stored.id) {
            let runtime_container = runtime_containers.iter().find(|c| c.id == stored.id);
            runtime_container
                .map(|c| c.status.clone())
                .unwrap_or_else(|| crate::types::AgentStatus::Running)
        } else {
            crate::types::AgentStatus::Stopped
        };

        merged_agents.push(crate::types::AgentContainer {
            id: stored.id,
            name: stored.name,
            status,
            config: stored.config,
            tailscale_ip: stored.tailscale_ip,  // Use persisted Tailscale IP
            resource_usage: None,
            project: None,
            tags: vec![],
            restart_policy: Default::default(),
            health_status: None,
            runtime: stored.runtime,
            gateway_port: stored
                .gateway_port
                .unwrap_or_else(crate::types::default_gateway_port),
        });
    }

    // Add any runtime containers that weren't in storage (shouldn't happen, but handle it)
    for runtime_container in runtime_containers {
        if !merged_agents.iter().any(|a| a.id == runtime_container.id) {
            merged_agents.push(runtime_container);
        }
    }

    tracing::info!("Total agents: {}", merged_agents.len());

    // Build index for O(1) agent lookups by ID (critical for scaling)
    let agent_index: std::collections::HashMap<String, usize> = merged_agents
        .iter()
        .enumerate()
        .map(|(idx, agent)| (agent.id.clone(), idx))
        .collect();

    // Initialize secrets manager
    let secrets = SecretsManager::new()?;
    tracing::info!("Secrets manager initialized");

    // Initialize snapshots manager
    let snapshots = SnapshotManager::new()?;
    tracing::info!("Snapshots manager initialized");

    // Initialize teams registry
    let teams = teams::TeamRegistry::new("teams");
    let teams_count = teams.load_all().await?;
    tracing::info!("Loaded {} teams", teams_count);

    // Load volumes
    let volumes = load_volumes(&data_dir);
    tracing::info!("Loaded {} volumes", volumes.len());

    // Initialize workflow registry
    let workflows = std::sync::Arc::new(tokio::sync::RwLock::new(workflow::WorkflowRegistry::new()));
    tracing::info!("Workflow registry initialized");

    // Initialize workflow executor
    let rpc_client = rpc::create_rpc_client();
    let containers_arc = std::sync::Arc::new(RwLock::new(merged_agents));
    let executor = std::sync::Arc::new(executor::WorkflowExecutor::new(
        std::sync::Arc::clone(&workflows),
        std::sync::Arc::clone(&containers_arc),
    ));
    tracing::info!("Workflow executor initialized");

    // Open chat DB — stored alongside the existing data files.
    let chat_db_path = data_dir.join("chat.db");
    let chat_db = std::sync::Arc::new(
        chat_db::ChatDb::open(&chat_db_path)
            .expect("failed to open chat database"),
    );
    tracing::info!("Chat database initialized at {:?}", chat_db_path);

    let state = Arc::new(AppState {
        config,
        containers: containers_arc,
        runtime,
        exo_runtime,
        templates: template_registry,
        andor: andor_client,
        secrets,
        snapshots,
        teams,
        api_keys: RwLock::new(load_api_keys(&data_dir)),
        data_dir,
        auth: RwLock::new(auth_manager),
        volumes: RwLock::new(volumes),
        container_ips: RwLock::new(std::collections::HashMap::new()),
        agent_index: RwLock::new(agent_index),
        rpc_client,
        workflows,
        executor,
        inference: inference_manager,
        chat_db,
    });

    // Create the protected API routes with auth middleware
    let protected_routes = Router::new()
        // Agent management - more specific routes MUST come before :id routes
        .route("/api/agents/:id/start", post(api::start_agent))
        .route("/api/agents/:id/stop", post(api::stop_agent))
        .route("/api/agents/:id/logs", get(api::get_logs))
        .route("/api/agents/:id/logs/stream", get(api::logs_websocket))
        .route("/api/agents/:id/metrics", get(api::get_metrics))
        .route("/api/agents/:id/health", post(api::run_health_check))
        .route(
            "/api/agents/:id/secrets",
            get(api::list_secrets).post(api::set_secret),
        )
        .route("/api/agents/:id/secrets/:name", delete(api::delete_secret))
        .route(
            "/api/agents/:id/snapshots",
            get(api::list_snapshots).post(api::create_snapshot),
        )
        .route(
            "/api/agents/:id/snapshots/:snapshot_id/restore",
            post(api::restore_snapshot),
        )
        .route(
            "/api/agents/:id/snapshots/:snapshot_id",
            delete(api::delete_snapshot),
        )
        .route("/api/agents/:id/export", get(api::export_agent))
        // Volume attachment for agents
        .route(
            "/api/agents/:id/volumes",
            get(api::list_agent_volumes)
                .post(api::attach_volume_to_agent),
        )
        .route(
            "/api/agents/:id/volumes/detach",
            post(api::detach_volume_from_agent),
        )
        // Generic :id routes come after all specific routes
        .route(
            "/api/agents/:id",
            get(api::get_agent)
                .put(api::update_agent)
                .delete(api::delete_agent),
        )
        .route(
            "/api/agents/:id/exec",
            post(api::exec_agent),
        )
        .route(
            "/api/agents/:id/terminal",
            get(api::terminal_websocket),
        )
        .route("/api/agents", get(api::list_agents).post(api::create_agent))
        // Batch operations
        .route("/api/agents/start-all", post(api::start_all))
        .route("/api/agents/stop-all", post(api::stop_all))
        // Service Discovery & Tailscale
        .route("/api/agents/:id/tailscale-ip", get(api::get_agent_tailscale_ip).put(api::update_agent_tailscale_ip))
        .route("/api/agents/tailscale", get(api::list_agents_with_tailscale))
        .route("/api/discovery/trigger", post(api::trigger_discovery))
        .route("/api/services/registry", get(api::get_service_registry))
        // Conversation History
        .route("/api/agents/:id/sessions", get(api::list_agent_sessions))
        .route("/api/agents/:id/sessions/:session_id", get(api::get_session_messages))
        // Agent RBAC assignments (admin/teacher only)
        .route(
            "/api/agents/:id/assignments",
            get(api::list_agent_assignments).post(api::assign_agent_user),
        )
        .route(
            "/api/agents/:id/assignments/:user_id",
            delete(api::unassign_agent_user),
        )
        // Agent-to-Agent Communication
        .route("/api/agents/:id/send", post(api::send_message))
        .route("/api/agents/:id/messages", get(api::get_agent_messages))
        .route("/api/agents/:id/ws/:target_id", get(api::websocket_proxy))
        // Global metrics
        .route("/api/metrics", get(api::get_all_metrics))
        .route("/api/system/stats", get(api::get_system_stats))
        // Templates
        .route("/api/templates", get(api::list_templates))
        // Tags
        .route("/api/tags", get(api::list_tags))
        // API Keys
        .route("/api/keys", get(api::list_api_keys).post(api::set_api_key))
        .route("/api/keys/:provider", delete(api::delete_api_key))
        // Volumes
        .route(
            "/api/volumes",
            get(api::list_volumes).post(api::create_volume),
        )
        .route(
            "/api/volumes/:id",
            get(api::get_volume)
                .put(api::update_volume)
                .delete(api::delete_volume),
        )
        // Projects
        .route(
            "/api/projects",
            get(api::list_projects).post(api::create_project),
        )
        // Teams
        .route("/api/teams", get(api::list_teams))
        .route("/api/teams/:id", get(api::get_team))
        .route("/api/teams/:id/classify", post(api::classify_message))
        // Team Role Assignments
        .route("/api/teams/:team_id/roles/:intent", post(api::assign_team_role))
        .route("/api/teams/:team_id/roles/:intent", get(api::get_team_role))
        .route("/api/teams/:team_id/roles/:intent", delete(api::remove_team_role))
        .route("/api/teams/:team_id/roles", get(api::list_team_roles))
        .route("/api/teams/:team_id/resolve/:intent", get(api::resolve_team_role))
        // Workflows
        .route("/api/workflows", post(api::create_workflow).get(api::list_workflows))
        .route("/api/workflows/:id", get(api::get_workflow))
        .route("/api/workflows/:id/execute", post(api::execute_workflow))
        .route("/api/workflows/:id/executions", get(api::list_workflow_executions))
        .route("/api/workflows/executions/:id", get(api::get_workflow_execution))
        // Import
        .route("/api/agents/import", post(api::import_agent))
        // Runtime status
        .route("/api/runtime/status", get(api::runtime_status))
        // Native Inference Service
        .route("/api/inference/status", get(api::inference_status))
        .route("/api/inference/start", post(api::inference_start))
        .route("/api/inference/stop", post(api::inference_stop))
        .route("/api/auth/refresh", post(auth::refresh));

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(api::health))
        .route("/terminal", get(api::terminal_page))
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .route("/auth/status", get(auth::auth_status))
        // Multi-user auth (chat_db-backed). Coexists with the legacy admin path.
        .route("/auth/user/register", post(auth::user_register))
        .route("/auth/user/login", post(auth::user_login))
        .route("/api/me", get(auth::me))
        // WebSocket chat routes (handle their own auth via query parameter)
        .route("/api/agents/:id/chat", get(api::chat_websocket))
        .route("/api/teams/:id/chat", get(api::team_chat_websocket));
    // Configure CORS to allow requests from Tauri app and development servers
    // Note: When using allow_credentials(true), we cannot use wildcard origin
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(
            |origin: &HeaderValue, _req_parts| {
                // Allow requests with no Origin header (same-origin, curl, etc.)
                if let Ok(origin_str) = origin.to_str() {
                    // Allow any localhost origin for development
                    if origin_str.starts_with("http://localhost:")
                        || origin_str.starts_with("http://127.0.0.1:")
                        || origin_str.starts_with("https://localhost")
                        || origin_str.starts_with("http://tauri.localhost")
                        || origin_str.starts_with("https://tauri.localhost")
                        || origin_str == "tauri://localhost"
                        || origin_str == "null"  // Some browsers send "null" for file://
                    {
                        return true;
                    }
                }
                // Allow requests with no Origin header (same-origin)
                false
            },
        ))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(86400));

    // Clone state for background task and shutdown handler
    let state_clone = state.clone();
    let state_shutdown = state.clone();

    // Static file serving - serve the web UI if configured
    let static_dir = state.config.static_dir.clone();
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .with_state(state.clone());

    let app = if let Some(dir) = static_dir {
        let static_path = std::path::PathBuf::from(&dir);
        if static_path.exists() {
            tracing::info!("📁 Serving static files from: {}", dir);
            app.fallback_service(ServeDir::new(&dir).append_index_html_on_directories(true))
        } else {
            tracing::warn!("⚠️  Static directory '{}' does not exist, skipping static file serving", dir);
            app
        }
    } else {
        app
    };

    // Get host and port from config
    let host = state.config.host.clone();
    let port = state.config.port;
    let addr = format!("{}:{}", host, port);
    tracing::info!("🦀 Claw Pen orchestrator listening on {}", addr);
    tracing::info!("🔐 JWT authentication enabled - all API endpoints require Bearer token");
    tracing::info!("   GET /auth/status to check auth configuration");
    tracing::info!("   POST /auth/login to authenticate");

    // Spawn background task to sync container states every 10 seconds
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            if let Err(e) = sync_container_states(&state_clone).await {
                tracing::warn!("Failed to sync container states: {}", e);
            }
            // Periodic health check for all running agents
            if let Err(e) = health_check_all_agents(&state_clone).await {
                tracing::warn!("Failed to health check agents: {}", e);
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutdown signal received, cleaning up...");
            if let Some(ref inference) = state_shutdown.inference {
                tracing::info!("Stopping native inference service...");
                if let Err(e) = inference.stop().await {
                    tracing::warn!("Error stopping inference service: {}", e);
                }
            }
        })
        .await?;

    Ok(())
}
