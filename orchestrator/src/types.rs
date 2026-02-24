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
    pub env_vars: std::collections::HashMap<String, String>,
}

fn default_memory() -> u32 { 1024 }
fn default_cpu() -> f32 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    // Cloud providers
    OpenAI,
    Anthropic,
    Gemini,
    Groq,

    // Moonshot AI / Kimi
    Kimi,
    // z.ai
    Zai,

    // Local providers (connect to model server)
    Ollama,
    LlamaCpp,
    Vllm,
    LmStudio,

    // Custom endpoint
    Custom {
        endpoint: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: f32,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    /// Template to use as base (e.g., "coding-assistant")
    #[serde(default)]
    pub template: Option<String>,
    /// Override template defaults. If no template, this is the full config.
    #[serde(default)]
    pub config: Option<PartialAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub config: Option<PartialAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialAgentConfig {
    pub llm_provider: Option<LlmProvider>,
    pub llm_model: Option<String>,
    pub memory_mb: Option<u32>,
    pub cpu_cores: Option<f32>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
}

impl AgentConfig {
    /// Apply partial overrides to this config
    pub fn apply(&mut self, partial: &PartialAgentConfig) {
        if let Some(ref provider) = partial.llm_provider {
            self.llm_provider = provider.clone();
        }
        if let Some(ref model) = partial.llm_model {
            self.llm_model = Some(model.clone());
        }
        if let Some(mem) = partial.memory_mb {
            self.memory_mb = mem;
        }
        if let Some(cores) = partial.cpu_cores {
            self.cpu_cores = cores;
        }
        if let Some(ref env) = partial.env_vars {
            self.env_vars.extend(env.clone());
        }
    }
}
