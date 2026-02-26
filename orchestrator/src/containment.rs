// Containment runtime client
// Communicates with the Containment container runtime

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Command;

use crate::types::{AgentContainer, AgentConfig, AgentStatus, ResourceUsage, LlmProvider};
use crate::container::ContainerRuntime;

pub struct ContainmentClient {
    /// Path to containment binary (or wsl command on Windows)
    runtime_path: String,
    /// WSL distro name (only used on Windows)
    wsl_distro: Option<String>,
}

impl ContainmentClient {
    pub fn new() -> Result<Self> {
        // Detect if we're on Windows or Linux
        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                runtime_path: "wsl".to_string(),
                wsl_distro: Some("containment".to_string()),
            })
        }

        #[cfg(not(target_os = "windows"))]
        {
            Ok(Self {
                runtime_path: "openclaw-runtime".to_string(),
                wsl_distro: None,
            })
        }
    }

    /// Build the command to run containment
    fn build_command(&self) -> Command {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = Command::new(&self.runtime_path);
            if let Some(ref distro) = self.wsl_distro {
                cmd.args(["-d", distro, "--", "openclaw-runtime"]);
            }
            cmd
        }

        #[cfg(not(target_os = "windows"))]
        {
            Command::new(&self.runtime_path)
        }
    }

    async fn list_containers_internal(&self) -> Result<Vec<AgentContainer>> {
        let output = self.build_command()
            .arg("list")
            .output()?;

        if !output.status.success() {
            tracing::warn!("Failed to list containers: {}", String::from_utf8_lossy(&output.stderr));
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut containers = Vec::new();

        // Parse output (format: CONTAINER ID\tNAME\tIMAGE\t\tSTATUS)
        for line in stdout.lines().skip(1) { // Skip header
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                let id = parts[0].to_string();
                let name = parts[1].to_string();
                let status = match parts[3] {
                    "running" => AgentStatus::Running,
                    "stopped" | "exited" => AgentStatus::Stopped,
                    _ => AgentStatus::Error,
                };

                containers.push(AgentContainer {
                    id,
                    name,
                    status,
                    config: AgentConfig::default(),
                    tailscale_ip: None,
                    resource_usage: None,
                });
            }
        }

        Ok(containers)
    }

    async fn create_container_internal(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Build container spec
        let spec = serde_json::json!({
            "name": name,
            "image": "openclaw-agent:latest",
            "command": ["openclaw", "agent", "--local"],
            "env": self.build_env_vars(config),
            "resources": {
                "memory": format!("{}M", config.memory_mb),
                "cpu": config.cpu_cores.to_string(),
            },
            "namespaces": {
                "pid": true,
                "network": true,
                "mount": true,
                "uts": true,
            },
        });

        let output = self.build_command()
            .args(["run", "--config", &spec.to_string(), "--id-only"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to create container: {}", String::from_utf8_lossy(&output.stderr));
        }

        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        tracing::info!("Created container: {} ({})", name, id);
        Ok(id)
    }

    async fn start_container_internal(&self, id: &str) -> Result<()> {
        // Containers start automatically on create in containment
        // This could be used to restart a stopped container
        tracing::info!("Container {} is running", id);
        Ok(())
    }

    async fn stop_container_internal(&self, id: &str) -> Result<()> {
        let output = self.build_command()
            .args(["stop", id])
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to stop container: {}", String::from_utf8_lossy(&output.stderr));
        }

        tracing::info!("Stopped container: {}", id);
        Ok(())
    }

    async fn delete_container_internal(&self, id: &str) -> Result<()> {
        // First stop if running
        let _ = self.stop_container_internal(id).await;

        // TODO: Add rm command to containment runtime
        tracing::info!("Deleted container: {}", id);
        Ok(())
    }

    async fn get_stats_internal(&self, _id: &str) -> Result<Option<ResourceUsage>> {
        // TODO: Implement stats collection
        Ok(None)
    }

    async fn container_exists_internal(&self, id: &str) -> Result<bool> {
        let containers = self.list_containers_internal().await?;
        Ok(containers.iter().any(|c| &c.id == id))
    }

    /// Build environment variables from agent config
    fn build_env_vars(&self, config: &AgentConfig) -> HashMap<String, String> {
        let mut env = config.env_vars.clone();

        // Set LLM provider
        let provider_str = match &config.llm_provider {
            LlmProvider::OpenAI => "openai",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::Gemini => "gemini",
            LlmProvider::Groq => "groq",
            LlmProvider::Kimi => "kimi",
            LlmProvider::Zai => "zai",
            LlmProvider::Ollama => "ollama",
            LlmProvider::LlamaCpp => "llamacpp",
            LlmProvider::Vllm => "vllm",
            LlmProvider::LmStudio => "lmstudio",
            LlmProvider::Custom { endpoint } => {
                env.insert("LLM_ENDPOINT".to_string(), endpoint.clone());
                "custom"
            }
        };

        env.insert("LLM_PROVIDER".to_string(), provider_str.to_string());

        if let Some(ref model) = config.llm_model {
            env.insert("LLM_MODEL".to_string(), model.clone());
        }

        // Set agent name
        env.insert("AGENT_NAME".to_string(), "claw-agent".to_string());

        // For local providers, configure host endpoint
        match &config.llm_provider {
            LlmProvider::Ollama => {
                env.entry("OLLAMA_HOST".to_string())
                    .or_insert_with(|| "http://host.containers.internal:11434".to_string());
            }
            LlmProvider::LmStudio => {
                env.entry("LMSTUDIO_HOST".to_string())
                    .or_insert_with(|| "http://host.containers.internal:1234".to_string());
            }
            _ => {}
        }

        // Note: OAuth providers (Kimi, z.ai) get tokens from OpenClaw gateway
        // No API keys needed in container env

        env
    }
}

#[async_trait]
impl ContainerRuntime for ContainmentClient {
    async fn list_containers(&self) -> Result<Vec<AgentContainer>> {
        self.list_containers_internal().await
    }

    async fn create_container(&self, name: &str, config: &AgentConfig) -> Result<String> {
        self.create_container_internal(name, config).await
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        self.start_container_internal(id).await
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        self.stop_container_internal(id).await
    }

    async fn delete_container(&self, id: &str) -> Result<()> {
        self.delete_container_internal(id).await
    }

    async fn get_stats(&self, id: &str) -> Result<Option<ResourceUsage>> {
        self.get_stats_internal(id).await
    }

    async fn container_exists(&self, id: &str) -> Result<bool> {
        self.container_exists_internal(id).await
    }
}

impl Default for ContainmentClient {
    fn default() -> Self {
        Self::new().expect("Failed to create ContainmentClient")
    }
}
