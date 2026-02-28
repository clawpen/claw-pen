use std::collections::HashMap;
mod andor;
mod api;
mod config;
mod container;
mod containment;
mod network;
mod secrets;
mod shared_memory;
mod snapshots;
mod storage;
mod teams;
mod templates;
mod types;

use axum::{
    routing::{delete, get, post},
    Router,
};
use container::ContainerRuntime;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

use crate::secrets::SecretsManager;
use crate::snapshots::SnapshotManager;

pub struct AppState {
    pub config: config::Config,
    pub containers: RwLock<Vec<types::AgentContainer>>,
    pub runtime: container::RuntimeClient,
    pub templates: templates::TemplateRegistry,
    pub andor: Option<andor::AndorClient>,
    pub secrets: SecretsManager,
    pub snapshots: SnapshotManager,
    pub teams: teams::TeamRegistry,
    pub api_keys: RwLock<HashMap<String, String>>,
    pub data_dir: std::path::PathBuf,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let config = config::load()?;
    let data_dir = std::path::PathBuf::from("/data/claw-pen/data");
    std::fs::create_dir_all(&data_dir).ok();
    tracing::info!("Loaded config: {:?}", config);

    // Load templates
    let template_registry = templates::TemplateRegistry::load()?;
    tracing::info!("Loaded {} templates", template_registry.list().len());

    // Initialize AndOR Bridge client if configured
    let andor_client = config.andor_bridge.as_ref().map(|cfg| {
        tracing::info!("AndOR Bridge configured at {}", cfg.url);
        andor::AndorClient::new(cfg.clone())
    });

    // Connect to runtime
    let runtime = container::RuntimeClient::new().await?;
    tracing::info!("Connected to container runtime");

    // Load persisted agents from storage
    let stored_agents = storage::load_agents().unwrap_or_default();
    tracing::info!("Loaded {} persisted agents", stored_agents.len());

    // Get runtime containers to update status
    let runtime_containers = runtime.list_containers().await?;
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
            tailscale_ip: None,
            resource_usage: None,
            project: None,
            tags: vec![],
            restart_policy: Default::default(),
            health_status: None,
        });
    }

    // Add any runtime containers that weren't in storage (shouldn't happen, but handle it)
    for runtime_container in runtime_containers {
        if !merged_agents.iter().any(|a| a.id == runtime_container.id) {
            merged_agents.push(runtime_container);
        }
    }

    tracing::info!("Total agents: {}", merged_agents.len());

    // Initialize secrets manager
    let secrets = SecretsManager::new()?;
    tracing::info!("Secrets manager initialized");

    // Initialize snapshots manager
    let snapshots = SnapshotManager::new()?;
    tracing::info!("Snapshots manager initialized");

    // Initialize teams registry
    let teams = teams::TeamRegistry::new("./teams");
    let teams_count = teams.load_all().await?;
    tracing::info!("Loaded {} teams", teams_count);

    let state = Arc::new(AppState {
        config,
        containers: RwLock::new(merged_agents),
        runtime,
        templates: template_registry,
        andor: andor_client,
        secrets,
        snapshots,
        teams,
        api_keys: RwLock::new(load_api_keys(&data_dir)),
        data_dir,
    });

    let app = Router::new()
        // Health check
        .route("/health", get(api::health))
        // Agent management - more specific routes MUST come before :id routes
        .route("/api/agents/:id/start", post(api::start_agent))
        .route("/api/agents/:id/stop", post(api::stop_agent))
        .route("/api/agents/:id/logs", get(api::get_logs))
        .route("/api/agents/:id/logs/stream", get(api::logs_websocket))
        .route("/api/agents/:id/chat", get(api::chat_websocket))
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
        // Generic :id routes come after all specific routes
        .route(
            "/api/agents/:id",
            get(api::get_agent)
                .put(api::update_agent)
                .delete(api::delete_agent),
        )
        .route("/api/agents", get(api::list_agents).post(api::create_agent))
        // Batch operations
        .route("/api/agents/start-all", post(api::start_all))
        .route("/api/agents/stop-all", post(api::stop_all))
        // Global metrics
        .route("/api/metrics", get(api::get_all_metrics))
        .route("/api/system/stats", get(api::get_system_stats))
        // Templates
        .route("/api/templates", get(api::list_templates))
        // API Keys
        .route("/api/keys", get(api::list_api_keys).post(api::set_api_key))
        .route("/api/keys/:provider", delete(api::delete_api_key))
        // Projects
        .route(
            "/api/projects",
            get(api::list_projects).post(api::create_project),
        )
        // Teams
        .route("/api/teams", get(api::list_teams))
        .route("/api/teams/:id", get(api::get_team))
        .route("/api/teams/:id/chat", get(api::team_chat_websocket))
        .route("/api/teams/:id/classify", post(api::classify_message))
        // Import
        .route("/api/agents/import", post(api::import_agent))
        // Runtime status
        .route("/api/runtime/status", get(api::runtime_status))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{}:{}", "0.0.0.0", 3000);
    tracing::info!("🦀 Claw Pen orchestrator listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
