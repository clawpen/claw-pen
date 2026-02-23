use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::config::AndorBridgeConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub display_name: String,
    pub triggers: Vec<String>,
    pub emoji: Option<String>,
}

pub struct AndorClient {
    config: AndorBridgeConfig,
    client: reqwest::Client,
}

impl AndorClient {
    pub fn new(config: AndorBridgeConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Register an agent with AndOR Bridge
    pub async fn register_agent(&self, registration: &AgentRegistration) -> Result<()> {
        let url = format!("{}/agents", self.config.url);
        
        let response = self.client
            .post(&url)
            .json(registration)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to register agent: {}", response.status());
        }

        tracing::info!("Registered agent '{}' with AndOR Bridge", registration.agent_id);
        Ok(())
    }

    /// Unregister an agent from AndOR Bridge
    pub async fn unregister_agent(&self, agent_id: &str) -> Result<()> {
        let url = format!("{}/agents/{}", self.config.url, agent_id);
        
        let response = self.client
            .delete(&url)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::warn!("Failed to unregister agent: {}", response.status());
        } else {
            tracing::info!("Unregistered agent '{}' from AndOR Bridge", agent_id);
        }

        Ok(())
    }

    /// List all registered agents
    pub async fn list_agents(&self) -> Result<Vec<AgentRegistration>> {
        let url = format!("{}/agents", self.config.url);
        
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let agents = response.json().await?;
        Ok(agents)
    }
}
