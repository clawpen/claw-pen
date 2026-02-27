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
    Kimi,
    Zai,
    Huggingface,
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

// === Team/Router Agent Types ===

/// A team of agents with a single router entry point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub router: RouterConfig,
    pub agents: HashMap<String, TeamAgent>,
    pub routing: HashMap<String, RoutingRule>,
    pub clarification: ClarificationConfig,
    pub responses: ResponseTemplates,
    pub created_at: String,
    pub status: TeamStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TeamStatus {
    Active,
    Inactive,
    Starting,
    Error,
}

/// Router agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub name: String,
    /// Classification mode: keyword, llm, or hybrid
    #[serde(default = "default_router_mode")]
    pub mode: RouterMode,
    /// Minimum confidence to route without clarification
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    /// Ask for clarification if confidence is low
    #[serde(default = "default_true")]
    pub clarify_on_low_confidence: bool,
}

fn default_router_mode() -> RouterMode {
    RouterMode::Hybrid
}

fn default_confidence_threshold() -> f32 {
    0.7
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouterMode {
    Keyword,
    Llm,
    Hybrid,
}

/// A specialist agent in a team
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAgent {
    /// The agent ID to route to
    pub agent: String,
    /// Description of what this agent handles
    pub description: String,
}

/// Routing rules for a specific intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Keywords that trigger this route
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Example messages for this intent
    #[serde(default)]
    pub examples: Vec<String>,
}

/// Clarification prompts when intent is unclear
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationConfig {
    #[serde(default = "default_clarification_prompts")]
    pub prompts: Vec<String>,
    #[serde(default = "default_options_format")]
    pub options_format: String,
}

fn default_clarification_prompts() -> Vec<String> {
    vec![
        "I'm not sure what you need. Are you asking about:".to_string(),
        "Could you clarify? This could be:".to_string(),
    ]
}

fn default_options_format() -> String {
    "• {intent}: {description}".to_string()
}

impl Default for ClarificationConfig {
    fn default() -> Self {
        Self {
            prompts: default_clarification_prompts(),
            options_format: default_options_format(),
        }
    }
}

/// Response templates for the router
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTemplates {
    #[serde(default = "default_routing_ack")]
    pub routing_ack: String,
    #[serde(default = "default_clarification_needed")]
    pub clarification_needed: String,
    #[serde(default)]
    pub agent_response_prefix: Option<String>,
}

fn default_routing_ack() -> String {
    "Let me pass this to {agent_name}...".to_string()
}

fn default_clarification_needed() -> String {
    "I need a bit more info to help you best.".to_string()
}

impl Default for ResponseTemplates {
    fn default() -> Self {
        Self {
            routing_ack: default_routing_ack(),
            clarification_needed: default_clarification_needed(),
            agent_response_prefix: None,
        }
    }
}

/// Request to create a new team
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: Option<String>,
    pub router_name: Option<String>,
    pub agents: HashMap<String, TeamAgent>,
    pub routing: HashMap<String, RoutingRule>,
}

/// Result of classifying a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// The detected intent (agent key)
    pub intent: String,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// Matched keywords (if any)
    pub matched_keywords: Vec<String>,
    /// Whether clarification is needed
    pub needs_clarification: bool,
}

/// A message being routed through a team
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedMessage {
    pub team_id: String,
    pub conversation_id: String,
    pub user_message: String,
    pub classification: Option<ClassificationResult>,
    pub target_agent: Option<String>,
    pub agent_response: Option<String>,
    pub timestamp: String,
}
