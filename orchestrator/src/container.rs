// Container runtime interface
// Abstracts over Docker and Containment runtimes

use anyhow::Result;
use async_trait::async_trait;

use crate::types::{AgentContainer, AgentConfig, ResourceUsage};
use crate::containment::ContainmentClient;

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
}

/// Default runtime client - uses Containment
pub struct RuntimeClient {
    inner: ContainmentClient,
}

impl RuntimeClient {
    pub async fn new() -> Result<Self> {
        let client = ContainmentClient::new()?;
        tracing::info!("Using Containment runtime");
        Ok(Self { inner: client })
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

    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        self.inner.get_stats(id).await
    }
}
