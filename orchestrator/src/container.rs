// Container runtime interface
// Abstracts over Docker and Containment runtimes

use anyhow::Result;
use async_trait::async_trait;

use crate::config::NetworkBackend;
use crate::containment::ContainmentClient;
use crate::types::{
    AgentConfig, AgentContainer, AgentStatus, LlmProvider, LogEntry, ResourceUsage,
};

/// Container runtime trait - abstracts over different backends
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// List all containers managed by this runtime
    async fn list_containers(&self) -> Result<Vec<AgentContainer>>;

    /// Create a new container
    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String>;

    /// Start a container
    async fn start_container(&self, id: &str) -> Result<()>;

    /// Stop a container
    async fn stop_container(&self, id: &str) -> Result<()>;

    /// Delete a container
    async fn delete_container(&self, id: &str) -> Result<()>;

    /// Get resource usage for a container
    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>>;

    /// Check if a container exists in the runtime
    async fn container_exists(&self, id: &str) -> Result<bool>;

    /// Get logs for a container
    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>>;

    /// Stream logs as they arrive
    async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry>;

    /// Run health check
    async fn health_check(&self, id: &str) -> Result<bool>;
}

/// Runtime client that uses either Containment or Docker based on availability
pub struct RuntimeClient {
    inner: RuntimeClientInner,
    network_backend: NetworkBackend,
    headscale_url: Option<String>,
    headscale_auth_key: Option<String>,
    headscale_namespace: Option<String>,
}

enum RuntimeClientInner {
    Containment(ContainmentClient),
    Docker(DockerClient),
}

impl RuntimeClient {
    pub async fn new() -> Result<Self> {
        // Try Docker first (easier setup for most users)
        match DockerClient::new().await {
            Ok(docker_client) => {
                tracing::info!("Using Docker runtime");
                return Ok(Self {
                    inner: RuntimeClientInner::Docker(docker_client),
                    network_backend: NetworkBackend::default(),
                    headscale_url: None,
                    headscale_auth_key: None,
                    headscale_namespace: None,
                });
            }
            Err(e) => {
                tracing::info!("Docker not available, trying Containment: {}", e);
            }
        }

        // Fallback to Containment
        let containment_client = ContainmentClient::new()?;
        tracing::info!("Using Containment runtime");
        Ok(Self {
            inner: RuntimeClientInner::Containment(containment_client),
            network_backend: NetworkBackend::default(),
            headscale_url: None,
            headscale_auth_key: None,
            headscale_namespace: None,
        })
    }

    /// Configure the network backend (called after loading config)
    pub fn with_network_config(
        mut self,
        network_backend: NetworkBackend,
        headscale_url: Option<String>,
        headscale_auth_key: Option<String>,
        headscale_namespace: Option<String>,
    ) -> Self {
        self.network_backend = network_backend.clone();
        self.headscale_url = headscale_url.clone();
        self.headscale_auth_key = headscale_auth_key.clone();
        self.headscale_namespace = headscale_namespace.clone();
        
        // If using Docker, update the inner client with network config
        if let RuntimeClientInner::Docker(ref docker) = self.inner {
            let new_docker = DockerClient::with_network_backend(
                docker.docker.clone(),
                network_backend,
                headscale_url,
                headscale_auth_key,
                headscale_namespace,
            );
            self.inner = RuntimeClientInner::Docker(new_docker);
        }
        
        self
    }
}

#[async_trait]
impl ContainerRuntime for RuntimeClient {
    async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.list_containers().await,
            RuntimeClientInner::Containment(client) => client.list_containers().await,
        }
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.create_container(name, config).await,
            RuntimeClientInner::Containment(client) => client.create_container(name, config).await,
        }
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.start_container(id).await,
            RuntimeClientInner::Containment(client) => client.start_container(id).await,
        }
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.stop_container(id).await,
            RuntimeClientInner::Containment(client) => client.stop_container(id).await,
        }
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.delete_container(id).await,
            RuntimeClientInner::Containment(client) => client.delete_container(id).await,
        }
    }

    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.get_stats(id).await,
            RuntimeClientInner::Containment(client) => client.get_stats(id).await,
        }
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.container_exists(id).await,
            RuntimeClientInner::Containment(client) => client.container_exists(id).await,
        }
    }

    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.get_logs(id, tail).await,
            RuntimeClientInner::Containment(client) => client.get_logs(id, tail).await,
        }
    }

    async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.stream_logs(id).await,
            RuntimeClientInner::Containment(client) => client.stream_logs(id).await,
        }
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.health_check(id).await,
            RuntimeClientInner::Containment(client) => client.health_check(id).await,
        }
    }
}

// ============================================================================
// Docker Runtime (alternative)
// ============================================================================

use bollard::container::{Config, CreateContainerOptions, ListContainersOptions};
use std::collections::HashMap;

/// Docker runtime client - uses Docker daemon via named pipe or socket
pub struct DockerClient {
    docker: bollard::Docker,
    network_backend: NetworkBackend,
    headscale_url: Option<String>,
    headscale_auth_key: Option<String>,
    headscale_namespace: Option<String>,
}

impl DockerClient {
    pub async fn new() -> Result<Self> {
        // Try to connect to Docker daemon
        #[cfg(target_os = "windows")]
        let docker = match bollard::Docker::connect_with_named_pipe(
            r"\\.\pipe\docker_engine",
            120, // timeout in seconds
            bollard::API_DEFAULT_VERSION,
        ) {
            Ok(d) => d,
            Err(e) => return Err(anyhow::anyhow!("Failed to connect to Docker: {}", e)),
        };

        #[cfg(not(target_os = "windows"))]
        let docker = match bollard::Docker::connect_with_socket(
            "/var/run/docker.sock",
            120, // timeout in seconds
            bollard::API_DEFAULT_VERSION,
        ) {
            Ok(d) => d,
            Err(e) => return Err(anyhow::anyhow!("Failed to connect to Docker: {}", e)),
        };

        tracing::info!("Connected to Docker runtime");
        Ok(Self { 
            docker,
            network_backend: NetworkBackend::default(),
            headscale_url: None,
            headscale_auth_key: None,
            headscale_namespace: None,
        })
    }

    /// Create a DockerClient with network backend configuration
    pub fn with_network_backend(
        docker: bollard::Docker,
        network_backend: NetworkBackend,
        headscale_url: Option<String>,
        headscale_auth_key: Option<String>,
        headscale_namespace: Option<String>,
    ) -> Self {
        Self {
            docker,
            network_backend,
            headscale_url,
            headscale_auth_key,
            headscale_namespace,
        }
    }

    /// Generate environment variables from config
    fn build_env_vars(config: &AgentConfig) -> Vec<String> {
        let mut env = Vec::new();

        // LLM provider configuration
        match config.llm_provider {
            LlmProvider::OpenAI => {
                if let Some(key) = config.env_vars.get("OPENAI_API_KEY") {
                    env.push(format!("OPENAI_API_KEY={}", key));
                }
            }
            LlmProvider::Anthropic => {
                if let Some(key) = config.env_vars.get("ANTHROPIC_API_KEY") {
                    env.push(format!("ANTHROPIC_API_KEY={}", key));
                }
            }
            LlmProvider::Gemini => {
                if let Some(key) = config.env_vars.get("GEMINI_API_KEY") {
                    env.push(format!("GEMINI_API_KEY={}", key));
                }
            }
            LlmProvider::Kimi => {
                if let Some(key) = config.env_vars.get("KIMI_API_KEY") {
                    env.push(format!("KIMI_API_KEY={}", key));
                }
            }
            LlmProvider::Zai => {
                if let Some(key) = config.env_vars.get("ZAI_API_KEY") {
                    env.push(format!("ZAI_API_KEY={}", key));
                }
            }
            LlmProvider::Huggingface => {
                if let Some(key) = config.env_vars.get("HF_TOKEN") {
                    env.push(format!("HF_TOKEN={}", key));
                }
            }
            LlmProvider::Ollama => {
                if let Some(endpoint) = config.env_vars.get("OLLAMA_ENDPOINT") {
                    env.push(format!("OLLAMA_ENDPOINT={}", endpoint));
                }
            }
            _ => {}
        }

        // Pass all custom env vars
        for (key, value) in &config.env_vars {
            if !key.starts_with("OPENAI_API_KEY")
                && !key.starts_with("ANTHROPIC_API_KEY")
                && !key.starts_with("GEMINI_API_KEY")
                && !key.starts_with("KIMI_API_KEY")
                && !key.starts_with("ZAI_API_KEY")
                && !key.starts_with("HF_TOKEN")
                && !key.starts_with("OLLAMA_ENDPOINT")
            {
                env.push(format!("{}={}", key, value));
            }
        }

        env
    }

    /// Generate environment variables for Headscale network backend
    /// These are passed to containers so they can join the Headscale mesh
    fn build_headscale_env_vars(&self) -> Vec<String> {
        let mut env = Vec::new();
        
        if matches!(self.network_backend, NetworkBackend::Headscale) {
            if let Some(ref url) = self.headscale_url {
                // Headscale server URL - containers use this with --login-server
                env.push(format!("HEADSCALE_URL={}", url));
            }
            if let Some(ref key) = self.headscale_auth_key {
                // Pre-auth key for automatic registration
                env.push(format!("HEADSCALE_AUTH_KEY={}", key));
            }
            // Namespace defaults to "claw-pen" in the container if not set
            if let Some(ref ns) = self.headscale_namespace {
                env.push(format!("HEADSCALE_NAMESPACE={}", ns));
            } else {
                env.push("HEADSCALE_NAMESPACE=claw-pen".to_string());
            }
            // Flag to indicate Headscale mode (container entrypoint can check this)
            env.push("TAILSCALE_LOGIN_SERVER=${HEADSCALE_URL}".to_string());
        }
        
        env
    }

    fn get_image_for_provider(provider: &LlmProvider) -> &'static str {
        match provider {
            LlmProvider::OpenAI => "openclaw-agent:latest",
            LlmProvider::Anthropic => "openclaw-agent:latest",
            LlmProvider::Gemini => "openclaw-agent:latest",
            LlmProvider::Kimi => "openclaw-agent:latest",
            LlmProvider::Zai => "openclaw-agent:latest",
            LlmProvider::Huggingface => "openclaw-agent:latest",
            LlmProvider::Ollama => "openclaw-agent:latest",
            LlmProvider::LlamaCpp => "openclaw-agent:latest",
            LlmProvider::Vllm => "openclaw-agent:latest",
            LlmProvider::Lmstudio => "openclaw-agent:latest",
            _ => "openclaw-agent:latest",
        }
    }

    /// Build labels HashMap for a container
    fn build_labels(name: &str, provider: &LlmProvider) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert("claw-pen-agent".to_string(), "true".to_string());
        labels.insert("claw-pen-agent-name".to_string(), name.to_string());
        labels.insert(
            "claw-pen-agent-provider".to_string(),
            format!("{:?}", provider).to_lowercase(),
        );
        labels
    }
}

#[async_trait]
impl ContainerRuntime for DockerClient {
    async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: HashMap::new(),
            limit: None,
            size: false,
        });

        let summaries = self.docker.list_containers(options).await?;

        let mut result = Vec::new();

        for container in summaries {
            // Only include containers that look like Claw Pen agents
            let is_claw_pen = container
                .labels
                .as_ref()
                .map(|l| l.contains_key("claw-pen-agent"))
                .unwrap_or(false);

            if is_claw_pen {
                let name = container
                    .names
                    .as_ref()
                    .and_then(|n| n.first())
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default();

                let id = container.id.unwrap_or_default();
                let state = container.state.unwrap_or_else(|| "unknown".to_string());
                let status = match state.as_str() {
                    "running" => AgentStatus::Running,
                    "exited" | "stopped" | "dead" => AgentStatus::Stopped,
                    "paused" => AgentStatus::Stopped,
                    "restarting" | "created" => AgentStatus::Starting,
                    _ => AgentStatus::Error,
                };

                result.push(AgentContainer {
                    id,
                    name,
                    status,
                    config: AgentConfig::default(),
                    tailscale_ip: None,
                    resource_usage: None,
                    project: None,
                    tags: vec![],
                    restart_policy: Default::default(),
                    health_status: None,
                });
            }
        }

        Ok(result)
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        let image = Self::get_image_for_provider(&config.llm_provider);
        let mut env = Self::build_env_vars(config);
        
        // Add Headscale environment variables if using Headscale backend
        let headscale_env = self.build_headscale_env_vars();
        env.extend(headscale_env);
        
        let labels = Self::build_labels(name, &config.llm_provider);

        // Container configuration
        let container_config = Config {
            image: Some(image.to_string()),
            env: Some(env),
            labels: Some(labels),
            host_config: Some(bollard::models::HostConfig {
                memory: Some((config.memory_mb * 1024 * 1024) as i64),
                nano_cpus: Some((config.cpu_cores * 1_000_000_000.0) as i64),
                ..Default::default()
            }),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name,
            platform: None,
        });

        let result = self
            .docker
            .create_container(options, container_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create container: {}", e))?;

        Ok(result.id)
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        self.docker
            .start_container::<String>(id, None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start container: {}", e))?;
        Ok(())
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        use bollard::container::StopContainerOptions;

        let options = Some(StopContainerOptions {
            t: 10, // timeout in seconds
        });

        self.docker
            .stop_container(id, options)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to stop container: {}", e))?;
        Ok(())
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        use bollard::container::RemoveContainerOptions;

        let options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        self.docker
            .remove_container(id, options)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete container: {}", e))?;
        Ok(())
    }

    async fn get_stats(&self, _id: &str) -> Result<Option<ResourceUsage>> {
        // TODO: Query container stats via Docker
        Ok(None)
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        match self.docker.inspect_container(id, None).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("Failed to check container: {}", e)),
        }
    }

    async fn get_logs(&self, _id: &str, _tail: usize) -> Result<Vec<LogEntry>> {
        // TODO: Implement Docker logs
        Ok(vec![])
    }

    async fn stream_logs(&self, _id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        // TODO: Implement Docker log streaming
        let (_tx, rx) = tokio::sync::mpsc::channel(10);
        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        // For Docker, check if container is running
        self.container_exists(id).await
    }
}
