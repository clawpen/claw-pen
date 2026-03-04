//! JWT Authentication Module for Claw Pen Orchestrator
//!
//! This module provides JWT-based authentication for all API endpoints.
//!
//! # Authentication Flow
//!
//! 1. On first run, a random JWT secret is generated and stored securely
//! 2. An admin password hash is stored (initially must be set via CLI or registration endpoint)
//! 3. Clients call `/auth/login` with password to get a JWT token
//! 4. All subsequent requests include `Authorization: Bearer <token>` header
//! 5. WebSocket connections pass token via `?token=<jwt>` query param
//!
//! # Endpoints
//!
//! - `POST /auth/login` - Authenticate and get JWT token (public)
//! - `POST /auth/register` - Register admin user (disabled by default, enable via ENABLE_REGISTRATION=true)
//! - `POST /auth/refresh` - Refresh an existing JWT token (requires auth)
//! - `GET /auth/status` - Check auth configuration status (public)

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;

use crate::AppState;

// === Configuration ===

/// JWT token expiration time in hours
const JWT_EXPIRATION_HOURS: i64 = 24;

/// Refresh token expiration time in days
const REFRESH_TOKEN_EXPIRATION_DAYS: i64 = 7;

/// JWT secret length in bytes (256 bits)
const JWT_SECRET_LENGTH: usize = 32;

// === Error Types ===

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid or expired token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Registration is disabled")]
    RegistrationDisabled,

    #[error("User already exists")]
    UserAlreadyExists,

    #[error("Password hash error: {0}")]
    HashError(String),

    #[error("JWT error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64Error(String),

    #[error("Missing authorization header")]
    MissingAuthHeader,

    #[error("Invalid authorization header format")]
    InvalidAuthHeaderFormat,
}

impl From<argon2::password_hash::Error> for AuthError {
    fn from(err: argon2::password_hash::Error) -> Self {
        AuthError::HashError(err.to_string())
    }
}

impl From<base64::DecodeError> for AuthError {
    fn from(err: base64::DecodeError) -> Self {
        AuthError::Base64Error(err.to_string())
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid credentials"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid or expired token"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::MissingAuthHeader => {
                (StatusCode::UNAUTHORIZED, "Missing authorization header")
            }
            AuthError::InvalidAuthHeaderFormat => (
                StatusCode::UNAUTHORIZED,
                "Invalid authorization header format",
            ),
            AuthError::RegistrationDisabled => (StatusCode::FORBIDDEN, "Registration is disabled"),
            AuthError::UserAlreadyExists => (StatusCode::CONFLICT, "User already exists"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

// === Token Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user identifier) - "admin" for single-user mode
    pub sub: String,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// Token type: "access" or "refresh"
    #[serde(rename = "type")]
    pub token_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub auth_enabled: bool,
    pub has_admin: bool,
    pub registration_enabled: bool,
}

// === Auth Manager ===

/// Manages authentication state and credentials
pub struct AuthManager {
    /// Path to the auth data directory
    data_dir: PathBuf,
    /// JWT secret for encoding/decoding
    jwt_secret: Vec<u8>,
    /// Hashed admin password
    admin_password_hash: Option<String>,
    /// Whether registration is enabled
    registration_enabled: bool,
}

impl AuthManager {
    /// Create a new AuthManager, initializing JWT secret if needed
    pub fn new(data_dir: &PathBuf) -> Result<Self, AuthError> {
        // Ensure data directory exists
        fs::create_dir_all(data_dir)?;

        let secret_path = data_dir.join("jwt_secret");
        let password_path = data_dir.join("admin_password");

        // Load or generate JWT secret
        let jwt_secret = if secret_path.exists() {
            let secret_b64 = fs::read_to_string(&secret_path)?;
            BASE64_STANDARD.decode(secret_b64.trim())?
        } else {
            let mut secret = vec![0u8; JWT_SECRET_LENGTH];
            OsRng.fill_bytes(&mut secret);

            // Store with restrictive permissions (0600)
            let secret_b64 = BASE64_STANDARD.encode(&secret);
            fs::write(&secret_path, &secret_b64)?;

            // Set restrictive permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&secret_path, fs::Permissions::from_mode(0o600))?;
            }

            tracing::info!("Generated new JWT secret and stored at {:?}", secret_path);
            secret
        };

        // Load admin password hash if exists
        let admin_password_hash = if password_path.exists() {
            let hash = fs::read_to_string(&password_path)?;
            Some(hash.trim().to_string())
        } else {
            None
        };

        // Check if registration is enabled via environment variable
        let registration_enabled = std::env::var("ENABLE_REGISTRATION")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        if registration_enabled {
            tracing::warn!("⚠️  Registration endpoint is ENABLED. Disable in production!");
        }

        Ok(Self {
            data_dir: data_dir.clone(),
            jwt_secret,
            admin_password_hash,
            registration_enabled,
        })
    }

    /// Check if an admin user exists
    pub fn has_admin(&self) -> bool {
        self.admin_password_hash.is_some()
    }

    /// Register admin user (only if registration is enabled or no admin exists)
    pub fn register(&mut self, password: &str) -> Result<(), AuthError> {
        // Allow registration if:
        // 1. Registration is explicitly enabled, OR
        // 2. No admin exists yet (first-time setup)
        if !self.registration_enabled && self.has_admin() {
            return Err(AuthError::RegistrationDisabled);
        }

        if self.has_admin() {
            return Err(AuthError::UserAlreadyExists);
        }

        // Hash the password with Argon2
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)?
            .to_string();

        // Store the hash
        let password_path = self.data_dir.join("admin_password");
        fs::write(&password_path, &password_hash)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&password_path, fs::Permissions::from_mode(0o600))?;
        }

        self.admin_password_hash = Some(password_hash);
        tracing::info!("Admin user registered successfully");

        Ok(())
    }

    /// Verify password and generate tokens
    pub fn login(&self, password: &str) -> Result<TokenResponse, AuthError> {
        let stored_hash = self
            .admin_password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;

        // Verify password
        let parsed_hash = PasswordHash::new(stored_hash)?;
        let argon2 = Argon2::default();

        argon2
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| AuthError::InvalidCredentials)?;

        // Generate tokens
        let access_token = self.generate_token("admin", "access", JWT_EXPIRATION_HOURS * 3600)?;
        let refresh_token = self.generate_token(
            "admin",
            "refresh",
            REFRESH_TOKEN_EXPIRATION_DAYS * 24 * 3600,
        )?;

        Ok(TokenResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: JWT_EXPIRATION_HOURS * 3600,
        })
    }

    /// Refresh an access token using a refresh token
    pub fn refresh(&self, refresh_token: &str) -> Result<TokenResponse, AuthError> {
        let claims = self.validate_token(refresh_token)?;

        if claims.token_type != "refresh" {
            return Err(AuthError::InvalidToken);
        }

        // Generate new tokens
        let access_token =
            self.generate_token(&claims.sub, "access", JWT_EXPIRATION_HOURS * 3600)?;
        let new_refresh_token = self.generate_token(
            &claims.sub,
            "refresh",
            REFRESH_TOKEN_EXPIRATION_DAYS * 24 * 3600,
        )?;

        Ok(TokenResponse {
            access_token,
            refresh_token: new_refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: JWT_EXPIRATION_HOURS * 3600,
        })
    }

    /// Generate a JWT token
    fn generate_token(
        &self,
        subject: &str,
        token_type: &str,
        expires_in_seconds: i64,
    ) -> Result<String, AuthError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: subject.to_string(),
            iat: now,
            exp: now + expires_in_seconds,
            token_type: token_type.to_string(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )?;

        Ok(token)
    }

    /// Validate a JWT token and return claims
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )?;

        Ok(token_data.claims)
    }

    /// Get the current auth status
    pub fn status(&self) -> AuthStatus {
        AuthStatus {
            auth_enabled: true,
            has_admin: self.has_admin(),
            registration_enabled: self.registration_enabled || !self.has_admin(),
        }
    }
}

// === API Handlers ===

/// POST /auth/login - Authenticate and get JWT tokens
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    let auth = state.auth.read().await;
    auth.login(&req.password).map(Json)
}

/// POST /auth/register - Register admin user
///
/// This endpoint is disabled by default for security. Enable by setting
/// ENABLE_REGISTRATION=true environment variable, or use the CLI to set
/// the initial password.
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<StatusCode, AuthError> {
    let mut auth = state.auth.write().await;
    auth.register(&req.password)?;
    Ok(StatusCode::CREATED)
}

/// POST /auth/refresh - Refresh access token
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    let auth = state.auth.read().await;
    auth.refresh(&req.refresh_token).map(Json)
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// GET /auth/status - Check auth configuration
pub async fn auth_status(State(state): State<Arc<AppState>>) -> Json<AuthStatus> {
    let auth = state.auth.read().await;
    Json(auth.status())
}

// === Middleware ===

/// JWT authentication middleware for HTTP requests
#[allow(dead_code)]
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Skip auth for health check
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    // Skip auth for auth endpoints themselves
    let path = request.uri().path();
    if path.starts_with("/auth/login")
        || path.starts_with("/auth/register")
        || path.starts_with("/auth/status")
    {
        return Ok(next.run(request).await);
    }

    // Extract token from Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingAuthHeader)?;

    // Parse "Bearer <token>"
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidAuthHeaderFormat)?;

    // Validate token
    let auth = state.auth.read().await;
    let claims = auth.validate_token(token)?;

    // Store claims in request extensions for handlers to use
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

/// Extract and validate JWT from WebSocket query parameter or first message
/// Returns the claims if valid, None if no token provided (for optional auth)
#[allow(dead_code)]
pub fn validate_ws_token(auth: &AuthManager, query_params: &str) -> Option<Claims> {
    // Parse query string
    for pair in query_params.split('&') {
        if let Some(token) = pair.strip_prefix("token=") {
            return auth.validate_token(token).ok();
        }
    }
    None
}

// === CLI Utilities ===

/// Set the admin password from CLI
/// Usage: claw-pen-orchestrator --set-password
pub fn cli_set_password(data_dir: &Path) -> Result<(), AuthError> {
    use std::io::{self, BufRead, Write};

    println!("Set admin password for Claw Pen Orchestrator");
    print!("Enter new password: ");
    io::stdout().flush()?;

    let stdin = io::stdin();
    let password = stdin
        .lock()
        .lines()
        .next()
        .ok_or_else(|| {
            AuthError::IoError(io::Error::new(io::ErrorKind::UnexpectedEof, "No input"))
        })?
        .map_err(AuthError::IoError)?;

    if password.len() < 8 {
        eprintln!("Password must be at least 8 characters");
        return Ok(());
    }

    // Create auth manager and set password
    let secret_path = data_dir.join("jwt_secret");
    let password_path = data_dir.join("admin_password");

    // Generate JWT secret if needed
    let _jwt_secret = if secret_path.exists() {
        let secret_b64 = fs::read_to_string(&secret_path)?;
        BASE64_STANDARD.decode(secret_b64.trim())?
    } else {
        let mut secret = vec![0u8; JWT_SECRET_LENGTH];
        OsRng.fill_bytes(&mut secret);
        let secret_b64 = BASE64_STANDARD.encode(&secret);
        fs::write(&secret_path, &secret_b64)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&secret_path, fs::Permissions::from_mode(0o600))?;
        }
        secret
    };

    // Hash and store password
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)?
        .to_string();

    fs::write(&password_path, &password_hash)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&password_path, fs::Permissions::from_mode(0o600))?;
    }

    println!("✓ Admin password set successfully");
    Ok(())
}
