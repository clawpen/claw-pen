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

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub deployment_mode: DeploymentMode,
    pub network_backend: NetworkBackend,
    pub runtime_socket: String,
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

pub fn load() -> anyhow::Result<Config> {
    dotenvy::dotenv().ok();

    let config = config::Config::builder()
        .set_default("deployment_mode", "windows-wsl")?
        .set_default("network_backend", "tailscale")?
        .set_default("runtime_socket", "/var/run/claw-pen.sock")?
        .set_default("tailscale_auth_key", None::<String>)?
        .set_default("headscale_url", None::<String>)?
        .set_default("headscale_auth_key", None::<String>)?
        .set_default("headscale_namespace", None::<String>)?
        .set_default("model_servers.ollama", None::<String>)?
        .set_default("model_servers.llama_cpp", None::<String>)?
        .set_default("model_servers.vllm", None::<String>)?
        .set_default("model_servers.lm_studio", None::<String>)?
        .set_default("andor_bridge", None::<String>)?
        .add_source(config::Environment::default().separator("__"))
        .build()?;

    Ok(config.try_deserialize()?)
}
