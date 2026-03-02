use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentMode {
    #[default]
    WindowsWsl,
    LinuxNative,
    AllWindows,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub deployment_mode: DeploymentMode,
    pub network_backend: NetworkBackend,
    pub runtime_socket: String,
    /// Container runtime to use: "docker" (default) or "exo"
    #[serde(default)]
    pub container_runtime: ContainerRuntimeType,
    /// Custom path to exo binary (defaults to "exo" in PATH)
    #[serde(default)]
    pub exo_path: Option<String>,
    pub tailscale_auth_key: Option<String>,
    /// Headscale server URL (e.g., https://mesh.yourcompany.com)
    /// Used when network_backend = "headscale"
    pub headscale_url: Option<String>,
    /// Headscale pre-authentication key for joining nodes
    /// Used when network_backend = "headscale"
    pub headscale_auth_key: Option<String>,
    /// Headscale namespace (defaults to "claw-pen" if not specified)
    pub headscale_namespace: Option<String>,
    pub model_servers: ModelServers,
    pub andor_bridge: Option<AndorBridgeConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AndorBridgeConfig {
    pub url: String,
    pub register_on_create: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelServers {
    pub ollama: Option<ModelServerConfig>,
    pub llama_cpp: Option<ModelServerConfig>,
    pub vllm: Option<ModelServerConfig>,
    pub lm_studio: Option<ModelServerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelServerConfig {
    pub endpoint: String,
    pub default_model: Option<String>,
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
        .set_default("deployment_mode", "windows-wsl")?
        .set_default("network_backend", "tailscale")?
        .set_default("runtime_socket", "/var/run/claw-pen.sock")?
        .set_default("container_runtime", "docker")?
        .set_default("exo_path", None::<String>)?
        .set_default("tailscale_auth_key", None::<String>)?
        .set_default("headscale_url", None::<String>)?
        .set_default("headscale_auth_key", None::<String>)?
        .set_default("headscale_namespace", None::<String>)?
        .set_default("model_servers.ollama", None::<String>)?
        .set_default("model_servers.llama_cpp", None::<String>)?
        .set_default("model_servers.vllm", None::<String>)?
        .set_default("model_servers.lm_studio", None::<String>)?
        .set_default("andor_bridge", None::<String>)?;

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
