//! Runtime configuration

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::env;

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Path to the exo binary
    pub exo_binary_path: Option<String>,

    /// Socket path for daemon communication
    pub socket_path: Option<String>,

    /// Data directory for agent state
    pub data_dir: String,

    /// Default gateway port
    pub gateway_port: u16,

    /// Whether to use daemon mode
    pub use_daemon: bool,

    /// WSL distro name (Windows only)
    #[cfg(windows)]
    pub wsl_distro: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            exo_binary_path: None,
            socket_path: Some("/tmp/exo-daemon.sock".to_string()),
            data_dir: Self::default_data_dir(),
            gateway_port: 18790,
            use_daemon: true,
            #[cfg(windows)]
            wsl_distro: "Ubuntu".to_string(),
        }
    }
}

impl RuntimeConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        if let Ok(path) = env::var("CLAW_PEN_EXO_BINARY") {
            config.exo_binary_path = Some(path);
        }

        if let Ok(path) = env::var("CLAW_PEN_SOCKET_PATH") {
            config.socket_path = Some(path);
        }

        if let Ok(dir) = env::var("CLAW_PEN_DATA_DIR") {
            config.data_dir = dir;
        }

        if let Ok(port) = env::var("CLAW_PEN_GATEWAY_PORT") {
            config.gateway_port = port.parse()
                .map_err(|_| anyhow!("Invalid gateway port: {}", port))?;
        }

        if let Ok(use_daemon) = env::var("CLAW_PEN_USE_DAEMON") {
            config.use_daemon = use_daemon != "0" && use_daemon.to_lowercase() != "false";
        }

        #[cfg(windows)]
        if let Ok(distro) = env::var("CLAW_PEN_WSL_DISTRO") {
            config.wsl_distro = distro;
        }

        Ok(config)
    }

    /// Get default data directory
    fn default_data_dir() -> String {
        #[cfg(unix)]
        {
            format!("{}/.claw-pen", env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        }

        #[cfg(windows)]
        {
            format!("{}\\.claw-pen",
                env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string()))
        }
    }

    /// Get the socket path for daemon communication
    pub fn socket_path(&self) -> &str {
        self.socket_path.as_deref().unwrap_or("/tmp/exo-daemon.sock")
    }

    /// Get the exo binary path
    pub fn exo_binary(&self) -> &str {
        self.exo_binary_path.as_deref().unwrap_or("exo")
    }
}

