//! Agent container management

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Default agent image for Claw Pen agents
const DEFAULT_AGENT_IMAGE: &str = "node:20-alpine";

/// Default gateway port for agent communication
const DEFAULT_GATEWAY_PORT: u16 = 18790;

/// Agent runtime trait - implemented by different container runtimes
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Start an agent container
    async fn start_agent(&self, spec: &AgentSpec) -> Result<AgentContainer, anyhow::Error>;

    /// Stop an agent container
    async fn stop_agent(&self, name: &str) -> Result<(), anyhow::Error>;

    /// Get agent status
    async fn agent_status(&self, name: &str) -> Result<ContainerStatus, anyhow::Error>;

    /// List all agent containers
    async fn list_agents(&self) -> Result<Vec<AgentContainer>, anyhow::Error>;

    /// Get agent logs
    async fn agent_logs(&self, name: &str, tail: Option<usize>) -> Result<String, anyhow::Error>;

    /// Execute a command in an agent container
    async fn exec_agent(&self, name: &str, command: &[String]) -> Result<String, anyhow::Error>;
}

/// Agent container specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    pub image: String,
    pub workdir: String,
    pub env: Vec<String>,
    pub command: Vec<String>,
    pub mounts: Vec<MountSpec>,
    pub gateway_port: u16,
}

impl AgentSpec {
    /// Create a new agent spec
    pub fn new(name: String) -> Self {
        Self {
            name,
            image: DEFAULT_AGENT_IMAGE.to_string(),
            workdir: "/agent".to_string(),
            env: vec![],
            command: vec![],
            mounts: vec![],
            gateway_port: DEFAULT_GATEWAY_PORT,
        }
    }

    /// Add an environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push(format!("{}={}", key.into(), value.into()));
        self
    }

    /// Add a volume mount
    pub fn with_mount(mut self, mount: MountSpec) -> Self {
        self.mounts.push(mount);
        self
    }

    /// Set the command
    pub fn with_command(mut self, command: Vec<String>) -> Self {
        self.command = command;
        self
    }

    /// Set the image
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }

    /// Set the gateway port
    pub fn with_gateway_port(mut self, port: u16) -> Self {
        self.gateway_port = port;
        self
    }

    /// Add multiple environment variables at once
    pub fn with_env_vars(mut self, env_vars: Vec<String>) -> Self {
        self.env.extend(env_vars);
        self
    }

    /// Add multiple mounts at once
    pub fn with_mounts(mut self, mounts: Vec<MountSpec>) -> Self {
        self.mounts.extend(mounts);
        self
    }
}

/// Volume mount specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

impl MountSpec {
    /// Create a new mount
    pub fn new(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            read_only: false,
        }
    }

    /// Set as read-only
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}

/// Agent container info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: ContainerStatus,
    pub image: String,
    pub gateway_port: u16,
    pub pid: Option<u32>,
}

/// Container status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
    Unknown,
}

impl ContainerStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped)
    }
}
