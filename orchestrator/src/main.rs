mod api;
mod config;
mod container;
mod network;
mod templates;
mod types;

use axum::{
    routing::{get, post, put, delete},
    Router,
};
use container::RuntimeClient;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub config: config::Config,
    pub containers: RwLock<Vec<types::AgentContainer>>,
    pub runtime: RuntimeClient,
    pub templates: templates::TemplateRegistry,
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

    // Connect to Docker
    let runtime = RuntimeClient::new().await?;
    tracing::info!("Connected to container runtime");

    // Load existing containers
    let existing = runtime.list_containers().await?;
    tracing::info!("Found {} existing Claw Pen agents", existing.len());

    let state = Arc::new(AppState {
        config,
        containers: RwLock::new(existing),
        runtime,
        templates: template_registry,
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
