// Container runtime interface
// Abstracts over Exo runtime

use anyhow::Result;
use async_trait::async_trait;

use crate::config::{ContainerRuntimeType, NetworkBackend};
use crate::exo_runtime::ExoRuntimeClient;
use crate::types::{
    AgentConfig, AgentContainer, LogEntry, ResourceUsage,
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

    /// Delete a container by name (for cleanup)
    async fn delete_container_by_name(&self, name: &str) -> Result<()>;

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

/// Runtime client that uses Exo for container management
#[derive(Clone)]
pub struct RuntimeClient {
    inner: ExoRuntimeClient,
    #[allow(dead_code)]
    network_backend: NetworkBackend,
    #[allow(dead_code)]
    headscale_url: Option<String>,
    #[allow(dead_code)]
    headscale_auth_key: Option<String>,
    #[allow(dead_code)]
    headscale_namespace: Option<String>,
}

impl RuntimeClient {
    pub async fn new() -> Result<Self> {
        Self::with_runtime(ContainerRuntimeType::default(), None).await
    }

    /// Create runtime client with specific runtime type
    pub async fn with_runtime(
        runtime_type: ContainerRuntimeType,
        exo_path: Option<String>,
    ) -> Result<Self> {
        // Exo is the only supported runtime now
        let exo_client = ExoRuntimeClient::new(exo_path.clone())?;
        
        match runtime_type {
            ContainerRuntimeType::Exo | ContainerRuntimeType::Docker => {
                tracing::info!("Using Exo runtime");
                Ok(Self {
                    inner: exo_client,
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
        self.network_backend = network_backend;
        self.headscale_url = headscale_url;
        self.headscale_auth_key = headscale_auth_key;
        self.headscale_namespace = headscale_namespace;
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
        self.inner.list_containers().await
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        self.inner.create_container(name, config).await
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        self.inner.start_container(id).await
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        self.inner.stop_container(id).await
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        self.inner.delete_container(id).await
    }

    async fn delete_container_by_name(&self, name: &str) -> Result<()> {
        self.inner.delete_container_by_name(name).await
    }

    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        self.inner.get_stats(id).await
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        self.inner.container_exists(id).await
    }

    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        self.inner.get_logs(id, tail).await
    }

    async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        self.inner.stream_logs(id).await
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        self.inner.health_check(id).await
    }
}
