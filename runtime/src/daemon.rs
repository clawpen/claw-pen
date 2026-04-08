//! Exo daemon client for container management
//!
//! Communicates with the exo daemon via Unix socket (Linux) or WSL2 (Windows)

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::RuntimeConfig;
use crate::agent::{AgentContainer, ContainerStatus};

// Local constants
const DEFAULT_GATEWAY_PORT: u16 = 18790;

/// Default exo daemon socket path
const DEFAULT_SOCKET_PATH: &str = "/tmp/exo-daemon.sock";

/// Default exo binary location
const DEFAULT_EXO_BINARY: &str = "exo";

/// Exo daemon client
pub struct ExoDaemonClient {
    config: RuntimeConfig,
    #[cfg(unix)]
    use_socat: bool,
}

impl ExoDaemonClient {
    /// Create a new daemon client
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        Ok(Self {
            config,
            #[cfg(unix)]
            use_socat: false,
        })
    }

    /// Ensure the exo daemon is running
    pub async fn ensure_daemon_running(&self) -> Result<()> {
        if self.is_daemon_running().await? {
            debug!("Exo daemon is already running");
            return Ok(());
        }

        info!("Starting exo daemon...");
        self.start_daemon().await?;

        // Wait for daemon to be ready
        for i in 0..30 {
            sleep(Duration::from_millis(500)).await;
            if self.is_daemon_running().await? {
                info!("Exo daemon is ready");
                return Ok(());
            }
            if i % 6 == 0 {
                debug!("Waiting for daemon... ({}/30)", i + 1);
            }
        }

        Err(anyhow!("Failed to start exo daemon"))
    }

    /// Check if the daemon is running
    pub async fn is_daemon_running(&self) -> Result<bool> {
        let socket_path = self.get_socket_path();

        #[cfg(unix)]
        {
            Ok(PathBuf::from(&socket_path).exists())
        }

        #[cfg(windows)]
        {
            // On Windows, check via WSL2
            let output = Command::new("wsl")
                .args(["--user", "root", "--", "test", "-f", &socket_path])
                .output()?;
            Ok(output.status.success())
        }
    }

    /// Start the exo daemon
    async fn start_daemon(&self) -> Result<()> {
        info!("Starting exo daemon in background");

        #[cfg(unix)]
        {
            let output = Command::new(DEFAULT_EXO_BINARY)
                .args(["daemon", "--foreground"])
                .spawn()?;

            // Give it a moment to start
            sleep(Duration::from_millis(500)).await;
            Ok(())
        }

        #[cfg(windows)]
        {
            // On Windows, start via WSL2
            let wsl_cmd = format!(
                "nohup {} daemon > /tmp/exo-daemon.log 2>&1 &",
                self.config.exo_binary_path.as_deref().unwrap_or(DEFAULT_EXO_BINARY)
            );

            let output = Command::new("wsl")
                .args(["--user", "root", "--", "bash", "-c", &wsl_cmd])
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("WSL daemon start warning: {}", stderr);
            }

            sleep(Duration::from_millis(500)).await;
            Ok(())
        }
    }

    /// Get the socket path for daemon communication
    fn get_socket_path(&self) -> String {
        self.config.socket_path.clone().unwrap_or_else(|| DEFAULT_SOCKET_PATH.to_string())
    }

    /// Send a request to the daemon
    async fn send_request(&self, request: &DaemonRequest) -> Result<DaemonResponse> {
        let socket_path = self.get_socket_path();
        let request_json = serde_json::to_string(request)?;

        debug!("Sending to daemon: {}", request_json);

        #[cfg(unix)]
        {
            use std::os::unix::net::UnixStream;
            use std::io::{BufRead, BufReader, Write};
            use std::time::Duration;

            let mut stream = UnixStream::connect(&socket_path)?;
            stream.set_read_timeout(Some(Duration::from_secs(30)))?;

            stream.write_all(request_json.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush()?;

            let mut reader = BufReader::new(stream.try_clone()?);
            let mut response_line = String::new();
            reader.read_line(&mut response_line)?;

            let response: DaemonResponse = serde_json::from_str(&response_line)?;
            Ok(response)
        }

        #[cfg(windows)]
        {
            // On Windows, use socat via WSL2
            let wsl_cmd = format!(
                "echo '{}' | socat - UNIX-CONNECT:{}",
                request_json.replace('"', r#"\""#),
                socket_path
            );

            let output = Command::new("wsl")
                .args(["--user", "root", "--", "bash", "-c", &wsl_cmd])
                .output()?;

            if !output.status.success() {
                return Err(anyhow!("Daemon request failed: {}", String::from_utf8_lossy(&output.stderr)));
            }

            let response_str = String::from_utf8_lossy(&output.stdout);
            let response: DaemonResponse = serde_json::from_str(&response_str)?;
            Ok(response)
        }
    }

    /// Stop the exo daemon
    pub async fn stop_daemon(&self) -> Result<()> {
        info!("Stopping exo daemon");

        #[cfg(unix)]
        {
            let _ = Command::new(DEFAULT_EXO_BINARY)
                .args(["daemon", "--stop"])
                .output();
        }

        #[cfg(windows)]
        {
            let _ = Command::new("wsl")
                .args([
                    "--user", "root", "--",
                    self.config.exo_binary_path.as_deref().unwrap_or(DEFAULT_EXO_BINARY),
                    "daemon", "--stop"
                ])
                .output();
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::agent::AgentRuntime for ExoDaemonClient {
    /// Start an agent container
    async fn start_agent(&self, spec: &crate::AgentSpec) -> Result<AgentContainer> {
        self.ensure_daemon_running().await?;

        let container_name = format!("agent-{}", spec.name);

        // Create container spec for daemon
        let daemon_spec = DaemonContainerSpec {
            name: container_name.clone(),
            image: spec.image.clone(),
            workdir: spec.workdir.clone(),
            env: spec.env.clone(),
            command: spec.command.clone(),
            mounts: spec.mounts.iter().map(|m| DaemonMountSpec {
                source: m.source.clone(),
                target: m.target.clone(),
                readonly: m.read_only,
            }).collect(),
            gateway_port: spec.gateway_port,
        };

        let request = DaemonRequest::Run { spec: daemon_spec };
        let response = self.send_request(&request).await?;

        match response {
            DaemonResponse::Ok { message } => {
                info!("Agent started: {}", message);
                Ok(AgentContainer {
                    id: container_name.clone(),
                    name: spec.name.clone(),
                    status: ContainerStatus::Running,
                    image: spec.image.clone(),
                    gateway_port: spec.gateway_port,
                    pid: None,
                })
            }
            DaemonResponse::Error { message } => {
                Err(anyhow!("Failed to start agent: {}", message))
            }
            _ => Err(anyhow!("Unexpected response from daemon")),
        }
    }

    /// Stop an agent container
    async fn stop_agent(&self, name: &str) -> Result<()> {
        let container_name = format!("agent-{}", name);
        let request = DaemonRequest::Stop { container_id: container_name };
        let response = self.send_request(&request).await?;

        match response {
            DaemonResponse::Ok { .. } => Ok(()),
            DaemonResponse::Error { message } => Err(anyhow!("Failed to stop agent: {}", message)),
            _ => Err(anyhow!("Unexpected response from daemon")),
        }
    }

    /// Get agent status
    async fn agent_status(&self, name: &str) -> Result<ContainerStatus> {
        let container_name = format!("agent-{}", name);
        let request = DaemonRequest::Status { container_id: container_name };
        let response = self.send_request(&request).await?;

        match response {
            DaemonResponse::Status { status, .. } => {
                match status.to_lowercase().as_str() {
                    "running" => Ok(ContainerStatus::Running),
                    "stopped" => Ok(ContainerStatus::Stopped),
                    "error" => Ok(ContainerStatus::Error),
                    _ => Ok(ContainerStatus::Unknown),
                }
            }
            _ => Ok(ContainerStatus::Unknown),
        }
    }

    /// List all agent containers
    async fn list_agents(&self) -> Result<Vec<AgentContainer>> {
        let request = DaemonRequest::List { all: true };
        let response = self.send_request(&request).await?;

        match response {
            DaemonResponse::List { containers } => {
                // Parse the JSON list of containers
                let container_list: Vec<serde_json::Value> = serde_json::from_str(&containers)?;

                let mut result = Vec::new();
                for c in container_list {
                    if let Some(name) = c.get("name").and_then(|n| n.as_str()) {
                        if name.starts_with("agent-") {
                            let agent_name = name.strip_prefix("agent-").unwrap_or(name);
                            let status = match c.get("status").and_then(|s| s.as_str()) {
                                Some("running") => ContainerStatus::Running,
                                Some("stopped") => ContainerStatus::Stopped,
                                Some("error") => ContainerStatus::Error,
                                _ => ContainerStatus::Unknown,
                            };

                            result.push(AgentContainer {
                                id: name.to_string(),
                                name: agent_name.to_string(),
                                status,
                                image: c.get("image").and_then(|i| i.as_str()).unwrap_or("unknown").to_string(),
                                gateway_port: DEFAULT_GATEWAY_PORT,
                                pid: None,
                            });
                        }
                    }
                }
                Ok(result)
            }
            _ => Ok(vec![]),
        }
    }

    /// Get agent logs
    async fn agent_logs(&self, name: &str, tail: Option<usize>) -> Result<String> {
        let container_name = format!("agent-{}", name);

        // Use exo logs command
        #[cfg(unix)]
        {
            let output = if let Some(n) = tail {
                Command::new(DEFAULT_EXO_BINARY)
                    .args(["logs", "--tail", &n.to_string(), &container_name])
                    .output()?
            } else {
                Command::new(DEFAULT_EXO_BINARY)
                    .args(["logs", &container_name])
                    .output()?
            };

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(anyhow!("Failed to get logs: {}", String::from_utf8_lossy(&output.stderr)))
            }
        }

        #[cfg(windows)]
        {
            let output = if let Some(n) = tail {
                Command::new("wsl")
                    .args(["--user", "root", "--", self.config.exo_binary_path.as_deref().unwrap_or(DEFAULT_EXO_BINARY), "logs", "--tail", &n.to_string(), &container_name])
                    .output()?
            } else {
                Command::new("wsl")
                    .args(["--user", "root", "--", self.config.exo_binary_path.as_deref().unwrap_or(DEFAULT_EXO_BINARY), "logs", &container_name])
                    .output()?
            };

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(anyhow!("Failed to get logs: {}", String::from_utf8_lossy(&output.stderr)))
            }
        }
    }

    /// Execute a command in an agent container
    async fn exec_agent(&self, name: &str, command: &[String]) -> Result<String> {
        let container_name = format!("agent-{}", name);

        #[cfg(unix)]
        {
            let mut args = vec!["exec", &container_name];
            args.extend(command.iter().map(|s| s.as_str()));

            let output = Command::new(DEFAULT_EXO_BINARY)
                .args(&args)
                .output()?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!("Exec failed: {}", stderr))
            }
        }

        #[cfg(windows)]
        {
            let mut args = vec!["exec", &container_name];
            args.extend(command.iter().map(|s| s.as_str()));

            let output = Command::new("wsl")
                .args(["--user", "root", "--", self.config.exo_binary_path.as_deref().unwrap_or(DEFAULT_EXO_BINARY)])
                .args(&args)
                .output()?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!("Exec failed: {}", stderr))
            }
        }
    }
}

/// Daemon request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum DaemonRequest {
    #[serde(rename = "run")]
    Run { spec: DaemonContainerSpec },

    #[serde(rename = "stop")]
    Stop { container_id: String },

    #[serde(rename = "list")]
    List { all: bool },

    #[serde(rename = "status")]
    Status { container_id: String },

    #[serde(rename = "ping")]
    Ping,
}

/// Daemon response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum DaemonResponse {
    #[serde(rename = "ok")]
    Ok { message: String },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "list")]
    List { containers: String },

    #[serde(rename = "status")]
    Status { status: String },

    #[serde(rename = "pong")]
    Pong,
}

/// Container specification for daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonContainerSpec {
    pub name: String,
    pub image: String,
    pub workdir: String,
    pub env: Vec<String>,
    pub command: Vec<String>,
    pub mounts: Vec<DaemonMountSpec>,
    pub gateway_port: u16,
}

/// Mount specification for daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonMountSpec {
    pub source: String,
    pub target: String,
    pub readonly: bool,
}
