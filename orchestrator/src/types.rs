use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: AgentStatus,
    pub config: AgentConfig,
    pub tailscale_ip: Option<String>,
    pub resource_usage: Option<ResourceUsage>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm_provider: LlmProvider,
    pub llm_model: Option<String>,
    pub memory_mb: u32,
    pub cpu_cores: f32,
    pub env_vars: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    // Cloud providers
    OpenAI,
    Anthropic,
    Gemini,
    Groq,
    
    // Local providers (connect to model server)
    Ollama,
    LlamaCpp,
    Vllm,
    
    // Custom endpoint
    Custom { endpoint: String },
}

impl Default for LlmProvider {
    fn default() -> Self {
        Self::OpenAI
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: f32,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub config: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub config: Option<PartialAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialAgentConfig {
    pub llm_provider: Option<String>,
    pub llm_model: Option<String>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<f32>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
}
