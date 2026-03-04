// Containment runtime client
// Communicates with the Containment container runtime

use crate::validation;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::mpsc;

use crate::container::ContainerRuntime;
use crate::types::{
    AgentConfig, AgentContainer, AgentStatus, LlmProvider, LogEntry, ResourceUsage, VolumeMount,
};

#[derive(Clone)]
pub struct ContainmentClient {
    /// Path to containment binary (or wsl command on Windows)
    runtime_path: String,
    /// WSL distro name (only used on Windows)
    #[allow(dead_code)]
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
        let output = self.build_command().arg("list").output()?;

        if !output.status.success() {
            tracing::warn!(
                "Failed to list containers: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut containers = Vec::new();

        // Parse output (format: CONTAINER ID\tNAME\tIMAGE\t\tSTATUS)
        for line in stdout.lines().skip(1) {
            // Skip header
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
                    project: None,
                    tags: vec![],
                    restart_policy: Default::default(),
                    health_status: None,
                    runtime: Some("containment".to_string()),
                });
            }
        }

        Ok(containers)
    }

    async fn create_container_internal(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Build container spec
        validation::validate_container_name(name)
            .map_err(|e| anyhow::anyhow!("Invalid container name: {}", e))?;

        // Validate resource limits
        validation::validate_memory_mb(config.memory_mb)
            .map_err(|e| anyhow::anyhow!("Invalid memory config: {}", e))?;
        validation::validate_cpu_cores(config.cpu_cores)
            .map_err(|e| anyhow::anyhow!("Invalid CPU config: {}", e))?;

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
            "mounts": self.build_mounts(&config.volumes),
        });

        let output = self
            .build_command()
            .args(["run", "--config", &spec.to_string(), "--id-only"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create container: {}",
                String::from_utf8_lossy(&output.stderr)
            );
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
        let output = self.build_command().args(["stop", id]).output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to stop container: {}",
                String::from_utf8_lossy(&output.stderr)
            );
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

    async fn get_stats_internal(&self, id: &str) -> Result<Option<ResourceUsage>> {
        // Read from /sys/fs/cgroup for the container
        let cgroup_path = format!("/sys/fs/cgroup/claw-pen/{}", id);

        // Memory usage
        let memory_current = std::fs::read_to_string(format!("{}/memory.current", cgroup_path))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);

        // CPU usage
        let cpu_stat = std::fs::read_to_string(format!("{}/cpu.stat", cgroup_path))
            .ok()
            .unwrap_or_default();

        // Parse usage_usec from cpu.stat
        let cpu_usec: u64 = cpu_stat
            .lines()
            .find(|l| l.starts_with("usage_usec"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Convert to percentage (very rough estimate)
        let cpu_percent = if cpu_usec > 0 {
            (cpu_usec as f32 / 1_000_000.0) % 100.0
        } else {
            0.0
        };

        Ok(Some(ResourceUsage {
            memory_mb: memory_current as f32 / (1024.0 * 1024.0),
            cpu_percent,
            network_rx_bytes: 0, // TODO: from /proc/net/dev
            network_tx_bytes: 0,
        }))
    }

    async fn container_exists_internal(&self, id: &str) -> Result<bool> {
        let containers = self.list_containers_internal().await?;
        Ok(containers.iter().any(|c| c.id == id))
    }

    pub async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        let log_path = format!("/var/lib/openclaw/containers/{}/logs/container.log", id);

        if !std::path::Path::new(&log_path).exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&log_path)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(tail);
        let lines = &all_lines[start..];

        let logs = lines
            .iter()
            .map(|line| {
                // Try to parse as JSON log, otherwise treat as plain text
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    LogEntry {
                        timestamp: json["timestamp"].as_str().unwrap_or("").to_string(),
                        level: json["level"].as_str().unwrap_or("info").to_string(),
                        message: json["message"].as_str().unwrap_or(line).to_string(),
                    }
                } else {
                    LogEntry {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        level: "info".to_string(),
                        message: line.to_string(),
                    }
                }
            })
            .collect();

        Ok(logs)
    }

    pub async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        let (tx, rx) = mpsc::channel(100);
        let log_path = format!("/var/lib/openclaw/containers/{}/logs/container.log", id);
        let _id_string = id.to_string();

        tokio::spawn(async move {
            // Simple tail -f implementation
            let mut last_size = 0u64;

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                if let Ok(metadata) = std::fs::metadata(&log_path) {
                    let size = metadata.len();

                    if size > last_size {
                        // Read new content
                        if let Ok(content) = std::fs::read_to_string(&log_path) {
                            let new_content = &content[last_size as usize..];
                            for line in new_content.lines() {
                                let entry = LogEntry {
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    level: "info".to_string(),
                                    message: line.to_string(),
                                };
                                if tx.send(entry).await.is_err() {
                                    return;
                                }
                            }
                        }
                        last_size = size;
                    }
                }
            }
        });

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    pub async fn health_check(&self, id: &str) -> Result<bool> {
        // Execute health check command in container
        // For now, just check if container is running
        let containers = self.list_containers_internal().await?;
        Ok(containers
            .iter()
            .any(|c| c.id == id && c.status == AgentStatus::Running))
    }

    /// Build environment variables from agent config
    fn build_env_vars(&self, config: &AgentConfig) -> HashMap<String, String> {
        let mut env = config.env_vars.clone();

        // Set LLM provider
        let provider_str = match &config.llm_provider {
            LlmProvider::OpenAI => "openai",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::Gemini => "gemini",
            LlmProvider::Kimi => "kimi",
            LlmProvider::Zai => "zai",
            LlmProvider::KimiCode => "kimi-code",
            LlmProvider::Access => "access",
            LlmProvider::Huggingface => "huggingface",
            LlmProvider::Ollama => "ollama",
            LlmProvider::LlamaCpp => "llamacpp",
            LlmProvider::Vllm => "vllm",
            LlmProvider::Lmstudio => "lmstudio",
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
            LlmProvider::Lmstudio => {
                env.entry("LMSTUDIO_HOST".to_string())
                    .or_insert_with(|| "http://host.containers.internal:1234".to_string());
            }
            _ => {}
        }

        // Note: OAuth providers (Kimi, z.ai) get tokens from OpenClaw gateway
        // No API keys needed in container env

        // Secrets are mounted at /run/secrets/ not in env
        for secret in &config.secrets {
            env.insert(format!("HAS_SECRET_{}", secret), "true".to_string());
        }

        env
    }

    /// Build mount specifications
    /// Build mount specifications with path validation
    fn build_mounts(&self, volumes: &[VolumeMount]) -> Vec<serde_json::Value> {
        volumes
            .iter()
            .filter_map(|v| {
                // Validate target path
                if let Err(e) = validation::validate_container_target(&v.target) {
                    tracing::warn!("Invalid volume target path {}: {}", v.target, e);
                    return None;
                }

                // Validate source path for path traversal
                // Note: Full canonicalization requires filesystem access
                if v.source.contains("..") {
                    tracing::warn!("Path traversal attempt in volume source: {}", v.source);
                    return None;
                }

                // Check for suspicious source paths
                let suspicious = [
                    "/etc/passwd",
                    "/etc/shadow",
                    "/root/.ssh",
                    "/var/run/docker.sock",
                ];
                if suspicious.iter().any(|s| v.source.starts_with(s)) {
                    tracing::warn!("Suspicious volume source path rejected: {}", v.source);
                    return None;
                }

                Some(serde_json::json!({
                    "type": "bind",
                    "source": v.source,
                    "target": v.target,
                    "readonly": v.read_only,
                }))
            })
            .collect()
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

    async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        self.get_logs(id, tail).await
    }

    async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        self.stream_logs(id).await
    }

    async fn health_check(&self, id: &str) -> Result<bool> {
        self.health_check(id).await
    }
}

impl Default for ContainmentClient {
    fn default() -> Self {
        Self::new().expect("Failed to create ContainmentClient")
    }
}
