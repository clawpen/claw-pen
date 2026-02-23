// Container runtime interface
// Currently: Docker (via bollard)
// Later: Swap for Jer's custom runtime

use anyhow::Result;
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, StartContainerOptions, StopContainerOptions,
    ListContainersOptions, InspectContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::ContainerStateStatusEnum;
use futures_util::StreamExt;

use crate::types::{AgentContainer, AgentConfig, AgentStatus, ResourceUsage, LlmProvider};

pub struct RuntimeClient {
    docker: Docker,
}

impl RuntimeClient {
    pub async fn new() -> Result<Self> {
        let docker = Docker::connect_with_socket_defaults()?;
        
        // Verify connection
        docker.ping().await?;
        tracing::info!("Connected to Docker daemon");
        
        Ok(Self { docker })
    }

    pub async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        let options = ListContainersOptions::<String> {
            all: true,
            filters: Some({
                let mut filters = std::collections::HashMap::new();
                filters.insert("label".to_string(), vec!["claw-pen.agent".to_string()]);
                filters
            }),
            ..Default::default()
        };

        let containers = self.docker.list_containers(Some(options)).await?;
        
        let mut agents = Vec::new();
        for c in containers {
            if let Some(id) = c.id {
                if let Some(names) = c.names {
                    if let Some(name) = names.first() {
                        let name = name.trim_start_matches('/').to_string();
                        
                        let status = match c.state {
                            Some(s) => match s.as_str() {
                                "running" => AgentStatus::Running,
                                "exited" | "created" => AgentStatus::Stopped,
                                "paused" => AgentStatus::Stopped,
                                _ => AgentStatus::Error,
                            },
                            None => AgentStatus::Error,
                        };

                        // Parse labels for config
                        let config = parse_labels_to_config(&c.labels);

                        agents.push(AgentContainer {
                            id,
                            name,
                            status,
                            config,
                            tailscale_ip: None, // TODO: extract from container
                            resource_usage: None,
                        });
                    }
                }
            }
        }

        Ok(agents)
    }

    pub async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Pull image first (OpenClaw base)
        let image = "node:20-alpine"; // TODO: Replace with OpenClaw agent image
        
        self.pull_image(image).await?;

        // Build container config
        let env_vars: Vec<String> = config.env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Add LLM provider config
        let llm_env = match &config.llm_provider {
            LlmProvider::OpenAI => vec![
                "LLM_PROVIDER=openai".to_string(),
                format!("OPENAI_API_KEY={}", config.env_vars.get("OPENAI_API_KEY").unwrap_or(&"".to_string())),
            ],
            LlmProvider::Anthropic => vec![
                "LLM_PROVIDER=anthropic".to_string(),
                format!("ANTHROPIC_API_KEY={}", config.env_vars.get("ANTHROPIC_API_KEY").unwrap_or(&"".to_string())),
            ],
            LlmProvider::Ollama => vec![
                "LLM_PROVIDER=ollama".to_string(),
                "OLLAMA_HOST=http://host.docker.internal:11434".to_string(),
            ],
            _ => vec![],
        };

        let mut env = env_vars;
        env.extend(llm_env);

        let container_config = Config {
            image: Some(image),
            env: Some(env),
            host_config: Some(bollard::service::HostConfig {
                memory: Some((config.memory_mb as i64) * 1024 * 1024), // Convert MB to bytes
                cpu_period: Some(100000),
                cpu_quota: Some((config.cpu_cores * 100000.0) as i64),
                extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
                ..Default::default()
            }),
            labels: Some({
                let mut labels = std::collections::HashMap::new();
                labels.insert("claw-pen.agent".to_string(), "true".to_string());
                labels.insert("claw-pen.name".to_string(), name.to_string());
                labels.insert("claw-pen.llm_provider".to_string(), format!("{:?}", config.llm_provider).to_lowercase());
                if let Some(ref model) = config.llm_model {
                    labels.insert("claw-pen.llm_model".to_string(), model.clone());
                }
                labels.insert("claw-pen.memory_mb".to_string(), config.memory_mb.to_string());
                labels.insert("claw-pen.cpu_cores".to_string(), config.cpu_cores.to_string());
                labels
            }),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: format!("claw-pen-{}", name),
            platform: None,
        };

        let response = self.docker.create_container(Some(options), container_config).await?;
        
        tracing::info!("Created container: {} ({})", name, response.id);
        Ok(response.id)
    }

    pub async fn start_container(&self, id: &str) -> Result<()> {
        self.docker.start_container(id, None::<StartContainerOptions<String>>).await?;
        tracing::info!("Started container: {}", id);
        Ok(())
    }

    pub async fn stop_container(&self, id: &str) -> Result<()> {
        self.docker.stop_container(id, Some(StopContainerOptions { t: 10 })).await?;
        tracing::info!("Stopped container: {}", id);
        Ok(())
    }

    pub async fn delete_container(&self, id: &str) -> Result<()> {
        self.docker.remove_container(id, None).await?;
        tracing::info!("Deleted container: {}", id);
        Ok(())
    }

    pub async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        // TODO: Implement stats collection via Docker stats API
        Ok(None)
    }

    async fn pull_image(&self, image: &str) -> Result<()> {
        let options = CreateImageOptions::<String> {
            from_image: image.to_string(),
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);
        
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(info) => {
                    if let Some(status) = info.status {
                        tracing::debug!("Pull image {}: {}", image, status);
                    }
                }
                Err(e) => {
                    tracing::warn!("Pull image warning: {}", e);
                }
            }
        }

        tracing::info!("Pulled image: {}", image);
        Ok(())
    }
}

fn parse_labels_to_config(labels: &Option<std::collections::HashMap<String, String>>) -> AgentConfig {
    let labels = labels.as_ref();
    
    let provider = labels
        .and_then(|l| l.get("claw-pen.llm_provider"))
        .map(|s| match s.as_str() {
            "openai" => LlmProvider::OpenAI,
            "anthropic" => LlmProvider::Anthropic,
            "gemini" => LlmProvider::Gemini,
            "groq" => LlmProvider::Groq,
            "ollama" => LlmProvider::Ollama,
            "llamacpp" => LlmProvider::LlamaCpp,
            "vllm" => LlmProvider::Vllm,
            _ => LlmProvider::OpenAI,
        })
        .unwrap_or(LlmProvider::OpenAI);

    AgentConfig {
        llm_provider: provider,
        llm_model: labels.and_then(|l| l.get("claw-pen.llm_model").cloned()),
        memory_mb: labels
            .and_then(|l| l.get("claw-pen.memory_mb"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024),
        cpu_cores: labels
            .and_then(|l| l.get("claw-pen.cpu_cores"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        env_vars: std::collections::HashMap::new(),
    }
}
