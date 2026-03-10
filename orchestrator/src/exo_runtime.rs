// Exo runtime client
// Communicates with the Exo container runtime via CLI

use crate::validation;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::mpsc;

use crate::container::ContainerRuntime;
use crate::types::{
    AgentConfig, AgentContainer, AgentStatus, AgentRuntime, LlmProvider, LogEntry, ResourceUsage, VolumeMount,
};

/// JSON structure for `exo list --json` output
#[derive(Debug, serde::Deserialize)]
struct ExoContainerList {
    containers: Vec<ExoContainer>,
}

#[derive(Debug, serde::Deserialize)]
struct ExoContainer {
    id: String,
    name: String,
    status: String,
    image: Option<String>,
    #[serde(default)]
    ports: Vec<ExoPortMapping>,
}

#[derive(Debug, serde::Deserialize)]
struct ExoPortMapping {
    container_port: u16,
    host_port: u16,
    protocol: String,
}

#[derive(Clone)]
pub struct ExoRuntimeClient {
    /// Path to exo binary
    exo_path: String,
}

impl ExoRuntimeClient {
    /// Create a new Exo runtime client
    ///
    /// # Arguments
    /// * `exo_path` - Optional custom path to exo binary. Defaults to "exo" in PATH.
    pub fn new(exo_path: Option<String>) -> Result<Self> {
        // Try provided path first, then check common locations
        let paths_to_try = if let Some(ref path) = exo_path {
            vec![path.clone()]
        } else {
            vec![
                "exo".to_string(),
                "/home/codi/Desktop/software/exo/target/release/exo".to_string(),
            ]
        };

        for path in &paths_to_try {
            if let Ok(output) = Command::new(path).arg("--version").output() {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    tracing::info!("Connected to Exo runtime at '{}': {}", path, version);
                    return Ok(Self { exo_path: path.clone() });
                }
            }
        }

        anyhow::bail!(
            "exo binary not found. Tried: {}. Ensure exo is installed and in PATH.",
            paths_to_try.join(", ")
        )
    }

    /// Get the path to the exo binary
    pub fn exo_path(&self) -> &str {
        &self.exo_path
    }

    async fn list_containers_internal(&self) -> Result<Vec<AgentContainer>> {
        let output = Command::new(&self.exo_path)
            .args(["list", "--json"])
            .output()?;

        if !output.status.success() {
            tracing::warn!(
                "Failed to list containers: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse JSON output from exo
        let exo_list: ExoContainerList = match serde_json::from_str(&stdout) {
            Ok(list) => list,
            Err(e) => {
                tracing::warn!("Failed to parse exo list JSON output: {}. Output: {}", e, stdout);
                return Ok(vec![]);
            }
        };

        let containers = exo_list.containers
            .into_iter()
            .map(|c| {
                let status = match c.status.to_lowercase().as_str() {
                    "running" => AgentStatus::Running,
                    "stopped" | "exited" => AgentStatus::Stopped,
                    "starting" | "created" => AgentStatus::Starting,
                    _ => AgentStatus::Error,
                };

                AgentContainer {
                    id: c.id,
                    name: c.name,
                    status,
                    config: AgentConfig::default(),
                    tailscale_ip: None,
                    resource_usage: None,
                    project: None,
                    tags: vec![],
                    restart_policy: Default::default(),
                    health_status: None,
                    runtime: Some("exo".to_string()),
                    agent_runtime: AgentRuntime::default(),
                    gateway_port: crate::types::default_gateway_port(),
                }
            })
            .collect();

        Ok(containers)
    }

    async fn create_container_internal(&self, name: &str, config: &AgentConfig) -> Result<String> {
        // Validate container name
        validation::validate_container_name(name)
            .map_err(|e| anyhow::anyhow!("Invalid container name: {}", e))?;

        // Validate resource limits
        validation::validate_memory_mb(config.memory_mb)
            .map_err(|e| anyhow::anyhow!("Invalid memory config: {}", e))?;
        validation::validate_cpu_cores(config.cpu_cores)
            .map_err(|e| anyhow::anyhow!("Invalid CPU config: {}", e))?;

        // Build args for exo run
        let mut args = vec![
            "run".to_string(),
            "--name".to_string(),
            name.to_string(),
            "-d".to_string(), // detached
        ];

        // Note: Memory and CPU limits not supported by exo runtime
        // These are validated but not passed to exo CLI
        // args.push("-m".to_string());
        // args.push(format!("{}M", config.memory_mb));
        // args.push("--cpus".to_string());
        // args.push(format!("{}", config.cpu_cores));

        // Add environment variables
        for (key, value) in self.build_env_vars(config) {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Add volume mounts
        for volume in &config.volumes {
            if validation::validate_container_target(&volume.target).is_ok() {
                let mount = if volume.read_only {
                    format!("{}:{}:ro", volume.source, volume.target)
                } else {
                    format!("{}:{}", volume.source, volume.target)
                };
                args.push("-v".to_string());
                args.push(mount);
            }
        }

        // Get gateway port from config
        let gateway_port: u16 = config
            .env_vars
            .get("PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(18790);

        // Add port mapping for gateway
        args.push("-p".to_string());
        args.push(format!("{}:{}:{}", gateway_port, gateway_port, "tcp"));

        // Select image based on agent runtime
        let default_runtime = AgentRuntime::default();
        let agent_runtime = config.agent_runtime.as_ref().unwrap_or(&default_runtime);
        let image = match agent_runtime {
            AgentRuntime::Openclaw => "openclaw-agent:latest",
            AgentRuntime::ExoNative => "exo-agent:latest",
        };
        args.push(image.to_string());

        // Default command for agent containers
        args.push("openclaw".to_string());
        args.push("agent".to_string());
        args.push("--local".to_string());

        let output = Command::new(&self.exo_path)
            .args(&args)
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
        let output = Command::new(&self.exo_path)
            .args(["start", id])
            .output()?;

        if !output.status.success() {
            tracing::warn!(
                "exo start returned non-zero (container may already be running): {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!("Started container: {}", id);
        Ok(())
    }

    async fn stop_container_internal(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.exo_path)
            .args(["stop", id])
            .output()?;

        if !output.status.success() {
            tracing::warn!(
                "exo stop returned non-zero (container may already be stopped): {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!("Stopped container: {}", id);
        Ok(())
    }

    async fn delete_container_internal(&self, id: &str) -> Result<()> {
        // First stop if running
        let _ = self.stop_container_internal(id).await;

        let output = Command::new(&self.exo_path)
            .args(["remove", id])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to remove container: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        tracing::info!("Deleted container: {}", id);
        Ok(())
    }

    async fn get_stats_internal(&self, _id: &str) -> Result<Option<ResourceUsage>> {
        // TODO: Implement stats collection via exo stats command when available
        Ok(None)
    }

    async fn container_exists_internal(&self, id: &str) -> Result<bool> {
        let containers = self.list_containers_internal().await?;
        Ok(containers.iter().any(|c| c.id == id || c.name == id))
    }

    pub async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        let output = Command::new(&self.exo_path)
            .args(["logs", id, "--tail", &tail.to_string()])
            .output()?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let logs = stdout
            .lines()
            .map(|line| LogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                level: "info".to_string(),
                message: line.to_string(),
            })
            .collect();

        Ok(logs)
    }

    pub async fn stream_logs(&self, id: &str) -> tokio_stream::wrappers::ReceiverStream<LogEntry> {
        let (tx, rx) = mpsc::channel(100);
        let exo_path = self.exo_path.clone();
        let id_string = id.to_string();

        tokio::spawn(async move {
            // Use exo logs --follow for streaming
            let mut child = match std::process::Command::new(&exo_path)
                .args(["logs", &id_string, "--follow"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to spawn exo logs --follow: {}", e);
                    return;
                }
            };

            // Read from stdout
            use std::io::{BufRead, BufReader};
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    let entry = LogEntry {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        level: "info".to_string(),
                        message: line,
                    };
                    if tx.send(entry).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }

    pub async fn health_check(&self, id: &str) -> Result<bool> {
        // Check if container exists and is running
        let containers = self.list_containers_internal().await?;
        Ok(containers
            .iter()
            .any(|c| (c.id == id || c.name == id) && c.status == AgentStatus::Running))
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

    /// Build volume mount specifications with path validation
    #[allow(dead_code)]
    fn build_mounts(&self, volumes: &[VolumeMount]) -> Vec<String> {
        volumes
            .iter()
            .filter_map(|v| {
                // Validate target path
                if let Err(e) = validation::validate_container_target(&v.target) {
                    tracing::warn!("Invalid volume target path {}: {}", v.target, e);
                    return None;
                }

                // Validate source path for path traversal
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

                if v.read_only {
                    Some(format!("{}:{}:ro", v.source, v.target))
                } else {
                    Some(format!("{}:{}", v.source, v.target))
                }
            })
            .collect()
    }
}

#[async_trait]
impl ContainerRuntime for ExoRuntimeClient {
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

    async fn delete_container_by_name(&self, name: &str) -> Result<()> {
        // For exo, name is the identifier
        self.delete_container_internal(name).await
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

impl Default for ExoRuntimeClient {
    fn default() -> Self {
        Self::new(None).expect("Failed to create ExoRuntimeClient")
    }
}
