use std::collections::HashMap;
use std::sync::Arc;

mod api;
mod auth;
mod chat_db;
mod config;
mod teams;

use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::{
    routing::{get, post, delete},
    Router,
};
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use crate::auth::AuthManager;
use crate::chat_db::ChatDb;

pub struct AppState {
    pub config: config::Config,
    pub teams: teams::TeamRegistry,
    pub api_keys: RwLock<HashMap<String, String>>,
    pub data_dir: std::path::PathBuf,
    pub auth: RwLock<AuthManager>,
    pub chat_db: Arc<ChatDb>,
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
    let data_dir = std::path::PathBuf::from("./data");
    std::fs::create_dir_all(&data_dir).ok();
    tracing::info!("Loaded config: {:?}", config);

    let auth_manager = AuthManager::new(&data_dir)?;
    if !auth_manager.has_admin() {
        tracing::warn!("⚠️  No admin password set. Use --set-password to set one.");
    }

    let api_keys = load_api_keys(&data_dir);
    let chat_db = Arc::new(ChatDb::open(&data_dir.join("chat.db"))?);

    let state = Arc::new(AppState {
        config: config.clone(),
        teams: teams::TeamRegistry::new(&data_dir)?,
        api_keys: RwLock::new(api_keys),
        data_dir: data_dir.clone(),
        auth: RwLock::new(auth_manager),
        chat_db,
    });

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/api/me", get(auth::me))
        .route("/auth/login", post(api::login))
        .route("/auth/user/login", post(auth::user_login))
        .route("/auth/register", post(api::register))
        .route("/auth/user/register", post(auth::user_register))
        .route("/auth/refresh", post(api::refresh_token))
        .route("/api/keys", get(api::list_api_keys).post(api::set_api_key))
        .route("/api/keys/:provider", delete(api::delete_api_key))
        .route("/api/conversations", get(api::list_conversations).post(api::create_conversation))
        .route("/api/conversations/:id", get(api::get_conversation).delete(api::delete_conversation))
        .route("/api/conversations/:id/messages", get(api::get_messages).post(api::send_message))
        .route("/api/chat/stream", post(api::chat_stream))
        .route("/api/teams", get(api::list_teams))
        .route("/api/teams/:id/roles", get(api::list_team_roles))
        .route("/api/admin/users", get(auth::admin_list_users))
        .route("/api/admin/users/pending", get(auth::admin_pending_users))
        .route("/api/admin/approve-user", post(auth::admin_approve_user))
        .route("/api/admin/create-user", post(auth::admin_create_user))
        .route("/ws/chat", get(api::chat_websocket))
        .fallback_service(ServeDir::new("./static-site"))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::any())
                .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
                .allow_headers([
                    header::CONTENT_TYPE,
                    header::AUTHORIZATION,
                    HeaderName::from_static("x-secret-word"),
                ]),
        )
        .with_state(state.clone());

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("🚀 Claw Pen Chat Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
