mod andor;
mod api;
mod config;
mod container;
mod containment;
mod network;
mod storage;
mod templates;
mod types;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use container::{ContainerRuntime, RuntimeClient};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub config: config::Config,
    pub containers: RwLock<Vec<types::AgentContainer>>,
    pub runtime: RuntimeClient,
    pub templates: templates::TemplateRegistry,
    pub andor: Option<andor::AndorClient>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::load()?;
    tracing::info!("Loaded config: {:?}", config);

    // Load templates
    let template_registry = templates::TemplateRegistry::load()?;
    tracing::info!("Loaded {} templates", template_registry.list().len());

    // Initialize AndOR Bridge client if configured
    let andor_client = config.andor_bridge.as_ref().map(|cfg| {
        tracing::info!("AndOR Bridge configured at {}", cfg.url);
        andor::AndorClient::new(cfg.clone())
    });

    // Connect to Docker
    let runtime = RuntimeClient::new().await?;
    tracing::info!("Connected to container runtime");

    // Load persisted agents from storage
    let stored_agents = storage::load_agents()
        .unwrap_or_default();
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
            let runtime_container = runtime_containers.iter()
                .find(|c| c.id == stored.id);
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
        });
    }

    // Add any runtime containers that weren't in storage (shouldn't happen, but handle it)
    for runtime_container in runtime_containers {
        if !merged_agents.iter().any(|a| a.id == runtime_container.id) {
            merged_agents.push(runtime_container);
        }
    }

    tracing::info!("Total agents: {}", merged_agents.len());

    let state = Arc::new(AppState {
        config,
        containers: RwLock::new(merged_agents),
        runtime,
        templates: template_registry,
        andor: andor_client,
    });

    let app = Router::new()
        // Health check
        .route("/health", get(api::health))
        // Agent management
        .route("/api/agents", get(api::list_agents))
        .route("/api/agents", post(api::create_agent))
        .route("/api/agents/{id}", get(api::get_agent))
        .route("/api/agents/{id}", put(api::update_agent))
        .route("/api/agents/{id}", delete(api::delete_agent))
        .route("/api/agents/{id}/start", post(api::start_agent))
        .route("/api/agents/{id}/stop", post(api::stop_agent))
        // Templates
        .route("/api/templates", get(api::list_templates))
        // Runtime info
        .route("/api/runtime/status", get(api::runtime_status))
        .layer(CorsLayer::new().allow_origin(Any))
        .with_state(state);

    let addr = format!("{}:{}", "0.0.0.0", 3000);
    tracing::info!("🦀 Claw Pen orchestrator listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
