use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentContainer {
    pub id: String,
    pub name: String,
    pub status: AgentStatus,
    pub config: AgentConfig,
    pub tailscale_ip: Option<String>,
    pub resource_usage: Option<ResourceUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub llm_provider: LlmProvider,
    pub llm_model: Option<String>,
    pub memory_mb: u32,
    pub cpu_cores: f32,
    pub env_vars: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    // Cloud
    OpenAI,
    Anthropic,
    Gemini,
    Kimi,
    Zai,
    Huggingface,
    // Local
    Ollama,
    LlamaCpp,
    Vllm,
    LmStudio,
}

impl LlmProvider {
    pub fn is_local(&self) -> bool {
        matches!(
            self,
            LlmProvider::Ollama | LlmProvider::LlamaCpp | LlmProvider::Vllm | LlmProvider::LmStudio
        )
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            LlmProvider::OpenAI => "OpenAI",
            LlmProvider::Anthropic => "Anthropic",
            LlmProvider::Gemini => "Gemini",
            LlmProvider::Kimi => "Kimi (Moonshot)",
            LlmProvider::Zai => "Zhipu AI (GLM)",
            LlmProvider::Huggingface => "Hugging Face",
            LlmProvider::Ollama => "Ollama (Local)",
            LlmProvider::LlamaCpp => "llama.cpp (Local)",
            LlmProvider::Vllm => "vLLM (Local)",
            LlmProvider::LmStudio => "LM Studio (Local)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceUsage {
    pub memory_mb: f32,
    pub cpu_percent: f32,
}
