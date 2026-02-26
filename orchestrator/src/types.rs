use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: AgentStatus,
    pub config: AgentConfig,
    pub tailscale_ip: Option<String>,
    pub resource_usage: Option<ResourceUsage>,
    /// Project/group this agent belongs to
    #[serde(default)]
    pub project: Option<String>,
    /// Tags for organization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Restart policy
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    /// Last health check result
    #[serde(default)]
    pub health_status: Option<HealthStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub llm_provider: LlmProvider,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    #[serde(default = "default_cpu")]
    pub cpu_cores: f32,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// Secret names to mount (will be available at /run/secrets/{name})
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Resource preset (overrides memory/cpu if set)
    #[serde(default)]
    pub preset: Option<ResourcePreset>,
    /// Auto-restart policy
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    /// Health check configuration
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
    /// Volumes to mount
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
}

fn default_memory() -> u32 {
    1024
}
fn default_cpu() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    OpenAI,
    Anthropic,
    Gemini,
    Groq,
    Kimi,
    Zai,
    Ollama,
    LlamaCpp,
    Vllm,
    Lmstudio,
    Custom {
        endpoint: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    #[default]
    Never,
    Always,
    OnFailure,
    UnlessStopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResourcePreset {
    Nano,   // 512MB, 0.5 CPU
    Micro,  // 1GB, 1 CPU
    Small,  // 2GB, 2 CPU
    Medium, // 4GB, 4 CPU
    Large,  // 8GB, 8 CPU
    Xlarge, // 16GB, 16 CPU
}

impl ResourcePreset {
    pub fn resources(&self) -> (u32, f32) {
        match self {
            ResourcePreset::Nano => (512, 0.5),
            ResourcePreset::Micro => (1024, 1.0),
            ResourcePreset::Small => (2048, 2.0),
            ResourcePreset::Medium => (4096, 4.0),
            ResourcePreset::Large => (8192, 8.0),
            ResourcePreset::Xlarge => (16384, 16.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Interval in seconds
    #[serde(default = "default_health_interval")]
    pub interval: u32,
    /// Timeout in seconds
    #[serde(default = "default_health_timeout")]
    pub timeout: u32,
    /// Number of retries before marking unhealthy
    #[serde(default = "default_health_retries")]
    pub retries: u32,
    /// Command to run for health check (default: openclaw --version)
    #[serde(default)]
    pub command: Option<Vec<String>>,
}

fn default_health_interval() -> u32 {
    30
}
fn default_health_timeout() -> u32 {
    10
}
fn default_health_retries() -> u32 {
    3
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            interval: 30,
            timeout: 10,
            retries: 3,
            command: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthStatus {
    pub healthy: bool,
    pub last_check: String, // ISO timestamp
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Name or path on host
    pub source: String,
    /// Path inside container
    pub target: String,
    /// Read-only mount
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: f32,
    pub cpu_percent: f32,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub config: Option<PartialAgentConfig>,
    /// Project to assign agent to
    #[serde(default)]
    pub project: Option<String>,
    /// Tags for organization
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub config: Option<PartialAgentConfig>,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialAgentConfig {
    pub llm_provider: Option<LlmProvider>,
    pub llm_model: Option<String>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<f32>,
    pub preset: Option<ResourcePreset>,
    pub env_vars: Option<HashMap<String, String>>,
    pub secrets: Option<Vec<String>>,
    pub restart_policy: Option<RestartPolicy>,
    pub health_check: Option<HealthCheck>,
    pub volumes: Option<Vec<VolumeMount>>,
}

// === Project/Group Management ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub agents: Vec<String>, // Agent IDs
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

// === Secrets Management ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInfo {
    pub name: String,
    pub created_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSecretRequest {
    pub name: String,
    pub value: String,
}

// === Logs ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

// === Snapshots ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub agent_id: String,
    pub created_at: String,
    pub size_bytes: u64,
}

// === Shared Memory Types (re-exported from shared_memory module) ===

#[allow(unused_imports)]
pub use crate::shared_memory::{
    AgentStatusEntry, Memory, MemorySearchResult, NewMemory, NewTask, SharedMemory,
    SharedMemoryConfig, SharedMemoryError, Task, TaskStatus, ORG_ALL, ORG_COMMON, ORG_DEFAULT,
};

impl AgentConfig {
    pub fn apply(&mut self, partial: &PartialAgentConfig) {
        if let Some(ref provider) = partial.llm_provider {
            self.llm_provider = provider.clone();
        }
        if let Some(ref model) = partial.llm_model {
            self.llm_model = Some(model.clone());
        }
        // Preset overrides individual settings
        if let Some(preset) = partial.preset {
            let (mem, cpu) = preset.resources();
            self.memory_mb = mem;
            self.cpu_cores = cpu;
            self.preset = Some(preset);
        } else {
            if let Some(mem) = partial.memory_mb {
                self.memory_mb = mem;
            }
            if let Some(cores) = partial.cpu_cores {
                self.cpu_cores = cores;
            }
        }
        if let Some(ref env) = partial.env_vars {
            self.env_vars.extend(env.clone());
        }
        if let Some(ref secrets) = partial.secrets {
            self.secrets = secrets.clone();
        }
        if let Some(policy) = partial.restart_policy {
            self.restart_policy = policy;
        }
        if let Some(ref health) = partial.health_check {
            self.health_check = Some(health.clone());
        }
        if let Some(ref volumes) = partial.volumes {
            self.volumes = volumes.clone();
        }
    }
}
