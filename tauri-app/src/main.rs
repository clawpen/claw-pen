// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aes_gcm::AeadCore;
use aes_gcm::AeadInPlace;
use aes_gcm::Aes256Gcm;
use aes_gcm::KeyInit;
use aes_gcm::Nonce as AesNonce;
use anyhow::Result;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub config: AgentConfig,
    pub tailscale_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm_provider: String,
    pub llm_model: Option<String>,
    pub memory_mb: u32,
    pub cpu_cores: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PartialAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialAgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentParams {
    pub name: String,
    pub template: Option<String>,
    pub provider: String,
    pub model: String,
    pub memory_mb: u32,
    pub cpu_cores: f32,
    pub env_vars: HashMap<String, String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub defaults: TemplateDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateDefaults {
    pub llm_provider: String,
    pub llm_model: String,
    pub memory_mb: u32,
    pub cpu_cores: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppConfig {
    pub orchestrator_url: String,
    pub has_completed_setup: bool,
    pub deployment_mode: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            orchestrator_url: "http://localhost:3000".to_string(),
            has_completed_setup: false,
            deployment_mode: "windows-wsl".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApiKey {
    pub provider: String,
    pub encrypted_key: String,
    pub created_at: String,
}

// ============================================================================
// Encryption
// ============================================================================

pub struct KeyManager {
    #[allow(dead_code)]
    key_path: PathBuf,
    master_key: Option<Vec<u8>>,
}

impl KeyManager {
    fn new() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("No config directory"))?
            .join("claw-pen");
        fs::create_dir_all(&config_dir)?;
        let key_path = config_dir.join("master.key");

        let master_key = if key_path.exists() {
            // Load existing key
            fs::read(&key_path)?
        } else {
            // Generate new key - 32 bytes for AES-256
            let mut key_bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key_bytes);
            fs::write(&key_path, key_bytes)?;
            key_bytes.to_vec()
        };

        Ok(Self {
            key_path,
            master_key: Some(master_key),
        })
    }

    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let key_bytes = &self.master_key.as_ref().unwrap()[..32];
        let cipher = Aes256Gcm::new(key_bytes.into());
        let nonce = Aes256Gcm::generate_nonce(&mut rand::rngs::OsRng);

        let mut buffer = plaintext.as_bytes().to_vec();
        cipher
            .encrypt_in_place(&nonce, b"", &mut buffer)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Combine nonce + ciphertext and encode as base64
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&buffer);
        Ok(base64::engine::general_purpose::STANDARD.encode(combined))
    }

    fn decrypt(&self, encrypted: &str) -> Result<String> {
        let key_bytes = &self.master_key.as_ref().unwrap()[..32];
        let cipher = Aes256Gcm::new(key_bytes.into());

        let combined = base64::engine::general_purpose::STANDARD.decode(encrypted)?;
        if combined.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data"));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = AesNonce::from_slice(nonce_bytes);

        let mut buffer = ciphertext.to_vec();
        cipher
            .decrypt_in_place(nonce, b"", &mut buffer)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        String::from_utf8(buffer).map_err(|e| anyhow::anyhow!("Invalid UTF-8: {}", e))
    }

    fn store_agent_key(&self, agent_id: &str, provider: &str, api_key: &str) -> Result<()> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("No config directory"))?
            .join("claw-pen")
            .join("keys");
        fs::create_dir_all(&config_dir)?;

        let key_file = config_dir.join(format!("{}.json", agent_id));
        let stored = StoredApiKey {
            provider: provider.to_string(),
            encrypted_key: self.encrypt(api_key)?,
            created_at: chrono_utc::now().to_rfc3339(),
        };
        fs::write(key_file, serde_json::to_string_pretty(&stored)?)?;
        Ok(())
    }

    fn get_agent_key(&self, agent_id: &str) -> Result<String> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("No config directory"))?
            .join("claw-pen")
            .join("keys");
        let key_file = config_dir.join(format!("{}.json", agent_id));

        if !key_file.exists() {
            return Err(anyhow::anyhow!("No API key found for agent"));
        }

        let content = fs::read_to_string(key_file)?;
        let stored: StoredApiKey = serde_json::from_str(&content)?;
        self.decrypt(&stored.encrypted_key)
    }

    fn delete_agent_key(&self, agent_id: &str) -> Result<()> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("No config directory"))?
            .join("claw-pen")
            .join("keys");
        let key_file = config_dir.join(format!("{}.json", agent_id));

        if key_file.exists() {
            fs::remove_file(key_file)?;
        }
        Ok(())
    }
}

// Simple chrono replacement for timestamps
mod chrono_utc {
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn now() -> DateTime {
        DateTime(())
    }

    pub struct DateTime(());

    impl DateTime {
        pub fn to_rfc3339(&self) -> String {
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("{}", duration)
        }
    }
}

// ============================================================================
// Configuration Management
// ============================================================================

fn get_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("claw-pen")
        .join("config.json")
}

fn load_config() -> Result<AppConfig> {
    let config_path = get_config_path();
    if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(AppConfig::default())
    }
}

fn save_config(config: &AppConfig) -> Result<()> {
    let config_path = get_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(config_path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

// ============================================================================
// API Client
// ============================================================================

struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            Err(anyhow::anyhow!("API error: {}", response.status()))
        }
    }

    async fn post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: serde::Serialize,
    {
        let response = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(response.json().await?)
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(anyhow::anyhow!("API error {}: {}", status, text))
        }
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let response = self
            .client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("API error: {}", response.status()))
        }
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
async fn get_config() -> Result<AppConfig, String> {
    load_config().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_app_config(
    orchestrator_url: String,
    has_completed_setup: bool,
    deployment_mode: String,
) -> Result<(), String> {
    let config = AppConfig {
        orchestrator_url,
        has_completed_setup,
        deployment_mode,
    };
    save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn health_check() -> Result<String, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health", config.orchestrator_url))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok("Orchestrator is running".to_string())
    } else {
        Err("Orchestrator not responding".to_string())
    }
}

#[tauri::command]
async fn list_agents() -> Result<Vec<AgentContainer>, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);
    client.get("/api/agents").await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_agent(id: String) -> Result<AgentContainer, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);
    client
        .get(&format!("/api/agents/{}", id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_agent(params: CreateAgentParams) -> Result<AgentContainer, String> {
    let name = params.name;
    let template = params.template;
    let provider = params.provider;
    let model = params.model;
    let memory_mb = params.memory_mb;
    let cpu_cores = params.cpu_cores;
    let env_vars = params.env_vars;
    let api_key = params.api_key;
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);

    // Store API key if provided
    let agent_id = format!("agent_{}", name.to_lowercase().replace(' ', "_"));

    if let Some(key) = api_key {
        if !key.is_empty() {
            let key_manager = KeyManager::new().map_err(|e| e.to_string())?;
            key_manager
                .store_agent_key(&agent_id, &provider, &key)
                .map_err(|e| e.to_string())?;

            // Add API key to env vars
            let mut env_vars_with_key = env_vars.clone();
            match provider.to_lowercase().as_str() {
                "openai" => env_vars_with_key.insert("OPENAI_API_KEY".to_string(), key),
                "anthropic" => env_vars_with_key.insert("ANTHROPIC_API_KEY".to_string(), key),
                "gemini" => env_vars_with_key.insert("GEMINI_API_KEY".to_string(), key),
                "groq" => env_vars_with_key.insert("GROQ_API_KEY".to_string(), key),
                _ => env_vars_with_key.insert("API_KEY".to_string(), key),
            };
        }
    }

    let req = CreateAgentRequest {
        name,
        template,
        config: Some(PartialAgentConfig {
            llm_provider: Some(provider),
            llm_model: if model.is_empty() { None } else { Some(model) },
            memory_mb: Some(memory_mb),
            cpu_cores: Some(cpu_cores),
            env_vars: if env_vars.is_empty() {
                None
            } else {
                Some(env_vars)
            },
        }),
    };

    client
        .post("/api/agents", &req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_agent(id: String) -> Result<(), String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);

    // Delete stored API key
    let key_manager = KeyManager::new().map_err(|e| e.to_string())?;
    let _ = key_manager.delete_agent_key(&id);

    client
        .delete(&format!("/api/agents/{}", id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_agent(id: String) -> Result<AgentContainer, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);

    // Retrieve and inject API key
    let key_manager = KeyManager::new().map_err(|e| e.to_string())?;
    if key_manager.get_agent_key(&id).is_ok() {
        // We need to update the agent with the API key before starting
        // This would require an update endpoint - for now, we'll pass it via env
        // TODO: Implement proper secret injection via Docker secrets
    }

    client
        .post(&format!("/api/agents/{}/start", id), &())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_agent(id: String) -> Result<AgentContainer, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);
    client
        .post(&format!("/api/agents/{}/stop", id), &())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_templates() -> Result<Vec<Template>, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = ApiClient::new(config.orchestrator_url);
    client
        .get("/api/templates")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_docker() -> Result<bool, String> {
    let config = load_config().map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/runtime/status", config.orchestrator_url))
        .send()
        .await
        .map_err(|e| e.to_string());

    match response {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
async fn get_stored_api_key(agent_id: String) -> Result<Option<String>, String> {
    let key_manager = KeyManager::new().map_err(|e| e.to_string())?;
    match key_manager.get_agent_key(&agent_id) {
        Ok(key) => Ok(Some(key)),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
async fn test_orchestrator_connection(url: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health", url))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response.status().is_success())
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .invoke_handler(tauri::generate_handler![
            // Config
            get_config,
            save_app_config,
            // Health
            health_check,
            check_docker,
            test_orchestrator_connection,
            // Agents
            list_agents,
            get_agent,
            create_agent,
            delete_agent,
            start_agent,
            stop_agent,
            // Templates
            list_templates,
            // API Keys
            get_stored_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
