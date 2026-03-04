// Container runtime interface
use crate::validation;
// Abstracts over Docker, Containment, and Exo runtimes

use anyhow::Result;
use async_trait::async_trait;

use crate::config::{ContainerRuntimeType, NetworkBackend};
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

/// Runtime client that uses Docker, Containment, or Exo based on configuration
#[derive(Clone)]
pub struct RuntimeClient {
    inner: RuntimeClientInner,
    network_backend: NetworkBackend,
    headscale_url: Option<String>,
    headscale_auth_key: Option<String>,
    headscale_namespace: Option<String>,
}

#[derive(Clone)]
enum RuntimeClientInner {
    Containment(ContainmentClient),
    Docker(DockerClient),
    Exo(ExoClient),
}

impl RuntimeClient {
    pub async fn new() -> Result<Self> {
        // Default to Docker for backward compatibility
        Self::with_runtime(ContainerRuntimeType::default(), None).await
    }

    /// Create runtime client with specific runtime type
    pub async fn with_runtime(
        runtime_type: ContainerRuntimeType,
        exo_path: Option<String>,
    ) -> Result<Self> {
        match runtime_type {
            ContainerRuntimeType::Exo => {
                let exo_client = ExoClient::new(exo_path)?;
                tracing::info!("Using Exo runtime");
                Ok(Self {
                    inner: RuntimeClientInner::Exo(exo_client),
                    network_backend: NetworkBackend::default(),
                    headscale_url: None,
                    headscale_auth_key: None,
                    headscale_namespace: None,
                })
            }
            ContainerRuntimeType::Docker => {
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
        }
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

    /// Clone the runtime client (used for secondary runtime instances)
    pub fn clone_runtime_client(&self) -> Self {
        self.clone()
    }
}

#[async_trait]
impl ContainerRuntime for RuntimeClient {
    async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.list_containers().await,
            RuntimeClientInner::Containment(client) => client.list_containers().await,
            RuntimeClientInner::Exo(client) => client.list_containers().await,
        }
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.create_container(name, config).await,
            RuntimeClientInner::Containment(client) => client.create_container(name, config).await,
            RuntimeClientInner::Exo(client) => client.create_container(name, config).await,
        }
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.start_container(id).await,
            RuntimeClientInner::Containment(client) => client.start_container(id).await,
            RuntimeClientInner::Exo(client) => client.start_container(id).await,
        }
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.stop_container(id).await,
            RuntimeClientInner::Containment(client) => client.stop_container(id).await,
            RuntimeClientInner::Exo(client) => client.stop_container(id).await,
        }
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.delete_container(id).await,
            RuntimeClientInner::Containment(client) => client.delete_container(id).await,
            RuntimeClientInner::Exo(client) => client.delete_container(id).await,
        }
    }

    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.get_stats(id).await,
            RuntimeClientInner::Containment(client) => client.get_stats(id).await,
            RuntimeClientInner::Exo(client) => client.get_stats(id).await,
        }
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.container_exists(id).await,
            RuntimeClientInner::Containment(client) => client.container_exists(id).await,
            RuntimeClientInner::Exo(client) => client.container_exists(id).await,
        }
    }

    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.get_logs(id, tail).await,
            RuntimeClientInner::Containment(client) => client.get_logs(id, tail).await,
            RuntimeClientInner::Exo(client) => client.get_logs(id, tail).await,
        }
    }

    async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.stream_logs(id).await,
            RuntimeClientInner::Containment(client) => client.stream_logs(id).await,
            RuntimeClientInner::Exo(client) => client.stream_logs(id).await,
        }
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        match &self.inner {
            RuntimeClientInner::Docker(client) => client.health_check(id).await,
            RuntimeClientInner::Containment(client) => client.health_check(id).await,
            RuntimeClientInner::Exo(client) => client.health_check(id).await,
        }
    }
}

// ============================================================================
// Docker Runtime (alternative)
// ============================================================================

use bollard::container::{Config, CreateContainerOptions, ListContainersOptions};
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions};
use std::collections::HashMap;
/// Default port for agent containers (internal communication)
const AGENT_INTERNAL_PORT: u16 = 8080;

/// Network name for Claw Pen containers (for isolation)
const CLAW_PEN_NETWORK: &str = "claw-pen-network";

/// Docker runtime client - uses Docker daemon via named pipe or socket
#[derive(Clone)]
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
            LlmProvider::KimiCode => {
                if let Some(key) = config.env_vars.get("KIMI_CODE_API_KEY") {
                    env.push(format!("KIMI_CODE_API_KEY={}", key));
                }
            }
            LlmProvider::Zai => {
                if let Some(key) = config.env_vars.get("ZAI_API_KEY") {
                    env.push(format!("ZAI_API_KEY={}", key));
                }
            }
            LlmProvider::Access => {
                if let Some(key) = config.env_vars.get("ACCESS_API_KEY") {
                    env.push(format!("ACCESS_API_KEY={}", key));
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
        // Pass provider and model to container
        env.push(format!("LLM_PROVIDER={:?}", config.llm_provider));
        if let Some(ref model) = config.llm_model {
            env.push(format!("LLM_MODEL={}", model));
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
            LlmProvider::KimiCode => "openclaw-agent:latest",
            LlmProvider::Access => "openclaw-agent:latest",
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

    /// Ensure the Claw Pen network exists for container isolation
    async fn ensure_network(&self) -> Result<()> {
        // Check if network exists
        let networks = self
            .docker
            .list_networks::<String>(None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list networks: {}", e))?;

        let exists = networks.iter().any(|n| {
            n.name
                .as_ref()
                .map(|name| name == CLAW_PEN_NETWORK)
                .unwrap_or(false)
        });

        if !exists {
            let create_opts = CreateNetworkOptions {
                name: CLAW_PEN_NETWORK,
                driver: "bridge",
                check_duplicate: true,
                internal: false, // Allow external access for API calls
                enable_ipv6: false,
                options: HashMap::new(),
                labels: HashMap::from([("claw-pen", "true"), ("purpose", "agent-isolation")]),
                ipam: bollard::models::Ipam {
                    config: Some(vec![bollard::models::IpamConfig {
                        subnet: Some("172.28.0.0/16".to_string()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
                attachable: true,
                ingress: false,
            };
            self.docker
                .create_network(create_opts)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create network: {}", e))?;

            tracing::info!("Created isolated network: {}", CLAW_PEN_NETWORK);
        }

        Ok(())
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
                    runtime: Some("docker".to_string()),
                });
            }
        }

        Ok(result)
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Validate container name to prevent command injection
        validation::validate_container_name(name)
            .map_err(|e| anyhow::anyhow!("Invalid container name: {}", e))?;

        // Validate resource limits
        validation::validate_memory_mb(config.memory_mb)
            .map_err(|e| anyhow::anyhow!("Invalid memory config: {}", e))?;
        validation::validate_cpu_cores(config.cpu_cores)
            .map_err(|e| anyhow::anyhow!("Invalid CPU config: {}", e))?;

        let image = Self::get_image_for_provider(&config.llm_provider);
        let mut env = Self::build_env_vars(config);

        // Add Headscale environment variables if using Headscale backend
        let headscale_env = self.build_headscale_env_vars();
        env.extend(headscale_env);

        let labels = Self::build_labels(name, &config.llm_provider);

        // Ensure the isolated network exists
        self.ensure_network().await?;

        // Build port bindings for bridge mode
        // Agent containers expose port 8080 internally for communication
        let port_bindings = HashMap::from([(
            format!("{}/tcp", AGENT_INTERNAL_PORT),
            Some(vec![bollard::models::PortBinding {
                host_ip: Some("127.0.0.1".to_string()), // Only bind to localhost for security
                host_port: None,                        // Let Docker assign a random port
            }]),
        )]);

        // Exposed ports (ports the container listens on)
        let exposed_ports =
            HashMap::from([(format!("{}/tcp", AGENT_INTERNAL_PORT), HashMap::new())]);

        // Container configuration with bridge network (isolated from host)
        let container_config = Config {
            image: Some(image.to_string()),
            env: Some(env),
            labels: Some(labels),
            exposed_ports: Some(exposed_ports),
            host_config: Some(bollard::models::HostConfig {
                memory: Some((config.memory_mb * 1024 * 1024) as i64),
                nano_cpus: Some((config.cpu_cores * 1_000_000_000.0) as i64),
                // Use bridge mode for network isolation instead of host mode
                network_mode: Some("bridge".to_string()),
                port_bindings: Some(port_bindings),
                // Security options
                security_opt: Some(vec!["no-new-privileges:true".to_string()]),
                // Prevent privilege escalation
                privileged: Some(false),
                // Read-only root filesystem where possible
                readonly_rootfs: Some(false), // Some agents may need to write
                // Drop all capabilities, add only what is needed
                cap_drop: Some(vec!["ALL".to_string()]),
                cap_add: Some(vec!["NET_BIND_SERVICE".to_string()]),
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

        // Connect to the isolated Claw Pen network
        let connect_opts = ConnectNetworkOptions {
            container: &result.id,
            endpoint_config: bollard::models::EndpointSettings {
                // No special endpoint config needed
                ..Default::default()
            },
        };

        if let Err(e) = self
            .docker
            .connect_network(CLAW_PEN_NETWORK, connect_opts)
            .await
        {
            tracing::warn!("Failed to connect container to isolated network: {}", e);
        }

        tracing::info!("Created container {} in isolated bridge network", result.id);
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

// ============================================================================
// Exo Runtime
// ============================================================================

use std::process::Command;

/// Exo runtime client - uses exo CLI for agent containers
#[derive(Clone)]
pub struct ExoClient {
    exo_path: String,
}

impl ExoClient {
    /// Create a new Exo client
    ///
    /// # Arguments
    /// * `exo_path` - Optional custom path to exo binary. Defaults to "exo" in PATH.
    pub fn new(exo_path: Option<String>) -> Result<Self> {
        let exo_path = exo_path.unwrap_or_else(|| "exo".to_string());

        // Verify exo is available
        let output = Command::new(&exo_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "exo binary not found at '{}': {}. Ensure exo is installed and in PATH.",
                    exo_path,
                    e
                )
            })?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exo binary at '{}' returned error",
                exo_path
            ));
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        tracing::info!("Connected to Exo runtime: {}", version);

        Ok(Self { exo_path })
    }

    /// Get the image name for a provider
    fn get_image_for_provider(provider: &LlmProvider) -> &'static str {
        // Exo uses the same images as Docker for compatibility
        match provider {
            LlmProvider::OpenAI => "openclaw-agent:latest",
            LlmProvider::Anthropic => "openclaw-agent:latest",
            LlmProvider::Gemini => "openclaw-agent:latest",
            LlmProvider::Kimi => "openclaw-agent:latest",
            LlmProvider::Zai => "openclaw-agent:latest",
            LlmProvider::Huggingface => "openclaw-agent:latest",
            LlmProvider::Ollama => "openclaw-agent:latest",
            LlmProvider::KimiCode => "openclaw-agent:latest",
            LlmProvider::Access => "openclaw-agent:latest",
            LlmProvider::LlamaCpp => "openclaw-agent:latest",
            LlmProvider::Vllm => "openclaw-agent:latest",
            LlmProvider::Lmstudio => "openclaw-agent:latest",
            _ => "openclaw-agent:latest",
        }
    }

    /// Build environment variables for exo run command
    fn build_env_args(config: &AgentConfig) -> Vec<String> {
        let mut args = Vec::new();

        // LLM provider configuration
        if let Some(key) = config.env_vars.get("OPENAI_API_KEY") {
            args.push("-e".to_string());
            args.push(format!("OPENAI_API_KEY={}", key));
        }
        if let Some(key) = config.env_vars.get("ANTHROPIC_API_KEY") {
            args.push("-e".to_string());
            args.push(format!("ANTHROPIC_API_KEY={}", key));
        }
        if let Some(key) = config.env_vars.get("GEMINI_API_KEY") {
            args.push("-e".to_string());
            args.push(format!("GEMINI_API_KEY={}", key));
        }
        if let Some(key) = config.env_vars.get("KIMI_API_KEY") {
            args.push("-e".to_string());
            args.push(format!("KIMI_API_KEY={}", key));
        }
        if let Some(key) = config.env_vars.get("ZAI_API_KEY") {
            args.push("-e".to_string());
            args.push(format!("ZAI_API_KEY={}", key));
        }
        if let Some(key) = config.env_vars.get("HF_TOKEN") {
            args.push("-e".to_string());
            args.push(format!("HF_TOKEN={}", key));
        }

        // Pass provider and model
        args.push("-e".to_string());
        args.push(format!("LLM_PROVIDER={:?}", config.llm_provider));
        if let Some(ref model) = config.llm_model {
            args.push("-e".to_string());
            args.push(format!("LLM_MODEL={}", model));
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
                args.push("-e".to_string());
                args.push(format!("{}={}", key, value));
            }
        }

        args
    }

    /// Parse container ID from exo ps output
    #[allow(dead_code)]
    fn parse_container_id(line: &str) -> Option<String> {
        // exo ps output format: ID  NAME  STATUS  IMAGE
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.first().map(|s| s.to_string())
    }
}

#[async_trait]
impl ContainerRuntime for ExoClient {
    async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        let output = Command::new(&self.exo_path)
            .args(["ps", "--all"])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list exo containers: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exo ps failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut containers = Vec::new();

        for line in stdout.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let id = parts.first().unwrap_or(&"").to_string();
                let name = parts.get(1).unwrap_or(&"").to_string();
                let status_str = parts.get(2).unwrap_or(&"").to_lowercase();

                let status = if status_str.contains("running") {
                    AgentStatus::Running
                } else if status_str.contains("stopped") || status_str.contains("exited") {
                    AgentStatus::Stopped
                } else if status_str.contains("starting") {
                    AgentStatus::Starting
                } else {
                    AgentStatus::Error
                };

                containers.push(AgentContainer {
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
                    runtime: Some("exo".to_string()),
                });
            }
        }

        Ok(containers)
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Validate container name
        validation::validate_container_name(name)
            .map_err(|e| anyhow::anyhow!("Invalid container name: {}", e))?;

        let image = Self::get_image_for_provider(&config.llm_provider);
        let mut args = vec![
            "run".to_string(),
            "--name".to_string(),
            name.to_string(),
            "--detach".to_string(),
        ];

        // Add memory limit (convert MB to bytes)
        args.push("--memory".to_string());
        args.push(format!("{}m", config.memory_mb));

        // Add CPU limit (as fraction of 1)
        args.push("--cpu".to_string());
        args.push(format!("{}", config.cpu_cores));

        // Add environment variables
        args.extend(Self::build_env_args(config));

        // Add image
        args.push(image.to_string());

        // Default command for agent containers
        args.push("openclaw".to_string());

        let output = Command::new(&self.exo_path)
            .args(&args)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to create exo container: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exo run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // exo run --detach returns the container ID
        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        tracing::info!("Created exo container: {} ({})", name, id);
        Ok(id)
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.exo_path)
            .args(["start", id])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to start exo container: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exo start failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        tracing::info!("Started exo container: {}", id);
        Ok(())
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.exo_path)
            .args(["stop", id])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to stop exo container: {}", e))?;

        if !output.status.success() {
            // Container might already be stopped
            tracing::warn!(
                "exo stop returned non-zero (container may already be stopped): {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!("Stopped exo container: {}", id);
        Ok(())
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.exo_path)
            .args(["rm", id])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to delete exo container: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exo rm failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        tracing::info!("Deleted exo container: {}", id);
        Ok(())
    }

    async fn get_stats(&self, _id: &str) -> Result<Option<ResourceUsage>> {
        // TODO: Implement stats collection for exo
        Ok(None)
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        let containers = self.list_containers().await?;
        Ok(containers.iter().any(|c| c.id == id || c.name == id))
    }

    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        let output = Command::new(&self.exo_path)
            .args(["logs", id, "--tail", &tail.to_string()])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to get exo logs: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut logs = Vec::new();

        for line in stdout.lines() {
            logs.push(LogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                level: "info".to_string(),
                message: line.to_string(),
            });
        }

        Ok(logs)
    }

    async fn stream_logs(&self, _id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        // TODO: Implement log streaming for exo
        let (_tx, rx) = tokio::sync::mpsc::channel(10);
        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        // For exo, check if container exists and is running
        let containers = self.list_containers().await?;
        Ok(containers
            .iter()
            .any(|c| (c.id == id || c.name == id) && c.status == AgentStatus::Running))
    }
}
