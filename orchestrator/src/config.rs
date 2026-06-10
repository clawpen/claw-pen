use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentMode {
    #[default]
    WindowsWsl,
    LinuxNative,
    AllWindows,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkBackend {
    #[default]
    Tailscale,
    Wireguard,
    Zerotier,
    Headscale,
    Local,
}

/// Container runtime selection
#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerRuntimeType {
    #[default]
    Docker,
    Exo,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default = "default_orchestrator_port")]
    pub port: u16,
    #[serde(default = "default_orchestrator_host")]
    pub host: String,
    #[serde(default = "default_static_dir")]
    pub static_dir: Option<String>,
    #[serde(default)]
    pub deployment_mode: DeploymentMode,
    #[serde(default)]
    pub network_backend: NetworkBackend,
    #[serde(default = "default_runtime_socket")]
    pub runtime_socket: String,
    /// Container runtime to use: "docker" (default) or "exo"
    #[serde(default)]
    pub container_runtime: ContainerRuntimeType,
    /// Custom path to exo binary (defaults to "exo" in PATH)
    #[serde(default)]
    pub exo_path: Option<String>,
    #[serde(default)]
    pub tailscale_auth_key: Option<String>,
    /// Headscale server URL (e.g., https://mesh.yourcompany.com)
    /// Used when network_backend = "headscale"
    #[serde(default)]
    pub headscale_url: Option<String>,
    /// Headscale pre-authentication key for joining nodes
    /// Used when network_backend = "headscale"
    #[serde(default)]
    pub headscale_auth_key: Option<String>,
    /// Headscale namespace (defaults to "claw-pen" if not specified)
    #[serde(default)]
    pub headscale_namespace: Option<String>,
    #[serde(default)]
    pub model_servers: ModelServers,
    #[serde(default)]
    pub andor_bridge: Option<AndorBridgeConfig>,
    /// Native inference service configuration
    #[serde(default)]
    pub native_inference: Option<NativeInferenceConfig>,
    /// Secret word for student registration. If set, students must provide this word to register.
    #[serde(default)]
    pub student_secret: Option<String>,
    /// Secret word for admin registration. If set, admins must provide this word to register.
    #[serde(default)]
    pub admin_secret: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
    .field("host", &self.host)
    .field("static_dir", &self.static_dir)
            .field("network_backend", &self.network_backend)
            .field("runtime_socket", &self.runtime_socket)
            .field("container_runtime", &self.container_runtime)
            .field("exo_path", &self.exo_path)
            .field("tailscale_auth_key", &self.tailscale_auth_key.as_ref().map(|_| "***REDACTED***"))
            .field("headscale_url", &self.headscale_url)
            .field("headscale_auth_key", &self.headscale_auth_key.as_ref().map(|_| "***REDACTED***"))
            .field("headscale_namespace", &self.headscale_namespace)
            .field("model_servers", &self.model_servers)
            .field("andor_bridge", &self.andor_bridge)
            .field("native_inference", &self.native_inference)
            .field("student_secret", &self.student_secret.as_ref().map(|_| "***REDACTED***"))
            .field("admin_secret", &self.admin_secret.as_ref().map(|_| "***REDACTED***"))
            .finish()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AndorBridgeConfig {
    pub url: String,
    pub register_on_create: Option<bool>,
}

/// Native inference service configuration (built-in GGUF model support)
#[derive(Debug, Deserialize, Clone)]
pub struct NativeInferenceConfig {
    /// Path to the GGUF model file
    pub model_path: String,
    /// Port for the inference API server
    #[serde(default = "default_inference_port")]
    pub port: u16,
    /// Maximum context window (tokens)
    #[serde(default = "default_context_window")]
    pub max_tokens: usize,
    /// Default temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Default top-p
    #[serde(default = "default_top_p")]
    pub top_p: f32,
}

fn default_inference_port() -> u16 {
    8765
}

fn default_context_window() -> usize {
    4096
}

fn default_temperature() -> f32 {
    0.7
}

fn default_top_p() -> f32 {
    0.9
}

fn default_orchestrator_port() -> u16 {
    3001
}

fn default_orchestrator_host() -> String {
    "127.0.0.1".to_string()
}

fn default_static_dir() -> Option<String> {
    Some("tauri-app/dist".to_string())
}

fn default_runtime_socket() -> String {
    "/var/run/claw-pen.sock".to_string()
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ModelServers {
    pub ollama: Option<ModelServerConfig>,
    pub llama_cpp: Option<ModelServerConfig>,
    pub vllm: Option<ModelServerConfig>,
    pub lm_studio: Option<ModelServerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ModelServerConfig {
    pub endpoint: String,
    pub default_model: Option<String>,
    /// API token for authentication (e.g., LM Studio requires Bearer token)
    pub api_token: Option<String>,
}

/// Config file locations to search (in order of priority)
const CONFIG_FILE_NAMES: &[&str] = &[
    "claw-pen.toml",
    "claw-pen.yaml",
    "claw-pen.yml",
    "claw-pen.json",
];

const CONFIG_DIRS: &[&str] = &[
    ".", // Current directory
    ".config/claw-pen",
    "~/.config/claw-pen",
    "/etc/claw-pen",
];

fn find_config_file() -> Option<std::path::PathBuf> {
    for dir in CONFIG_DIRS {
        let dir_path = if dir.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                home.join(dir.strip_prefix("~/").unwrap_or(""))
            } else {
                continue;
            }
        } else {
            std::path::PathBuf::from(dir)
        };

        for name in CONFIG_FILE_NAMES {
            let path = dir_path.join(name);
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

pub fn load() -> anyhow::Result<Config> {
    dotenvy::dotenv().ok();

    let mut builder = config::Config::builder()
        .set_default("port", 3001)?
        .set_default("host", "127.0.0.1")?
        .set_default("static-dir", None::<String>)?
        .set_default("deployment-mode", "windows-wsl")?
        .set_default("network-backend", "tailscale")?
        .set_default("runtime-socket", "/var/run/claw-pen.sock")?
        .set_default("container-runtime", "docker")?
        .set_default("exo-path", None::<String>)?
        .set_default("tailscale-auth-key", None::<String>)?
        .set_default("headscale-url", None::<String>)?
        .set_default("headscale-auth-key", None::<String>)?
        .set_default("headscale-namespace", None::<String>)?
        .set_default("student-secret", None::<String>)?
        .set_default("admin-secret", None::<String>)?;

    // Load from config file if found
    if let Some(config_path) = find_config_file() {
        tracing::info!("Loading config from: {}", config_path.display());
        let extension = config_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("toml");

        builder = builder.add_source(match extension {
            "yaml" | "yml" => config::File::from(config_path).format(config::FileFormat::Yaml),
            "json" => config::File::from(config_path).format(config::FileFormat::Json),
            _ => config::File::from(config_path).format(config::FileFormat::Toml),
        });
    }

    // Environment variables override file config
    let config = builder
        .add_source(config::Environment::default().separator("__"))
        .build()?;

    Ok(config.try_deserialize()?)
}
