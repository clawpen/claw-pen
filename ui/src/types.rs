use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
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

// === Team Types ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub router: RouterConfig,
    pub agents: std::collections::HashMap<String, TeamAgent>,
    pub routing: std::collections::HashMap<String, RoutingRule>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouterConfig {
    pub name: String,
    pub mode: RouterMode,
    pub confidence_threshold: f32,
    pub clarify_on_low_confidence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouterMode {
    Keyword,
    Llm,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamAgent {
    pub agent: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutingRule {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamRoleAssignment {
    pub id: String,
    pub team_id: String,
    pub intent: String,
    pub agent_id: String,
    pub assigned_at: String,
    pub assigned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRoleRequest {
    pub agent_id: String,
    pub assigned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub intent: String,
    pub confidence: f32,
    pub matched_keywords: Vec<String>,
    pub needs_clarification: bool,
}

