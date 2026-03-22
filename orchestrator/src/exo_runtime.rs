// Exo runtime client
// Communicates with the Exo container runtime via CLI

use crate::validation;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Command as StdCommand;
use tokio::process::Command;
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
                "/data/software/exo/target/release/exo".to_string(),
                "/home/codi/.local/bin/exo".to_string(),
            ]
        };

        for path in &paths_to_try {
            // Use std::process::Command for synchronous version check at startup
            if let Ok(output) = std::process::Command::new(path).arg("--version").output() {
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
        // Add timeout to list command to prevent hanging
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            Command::new(&self.exo_path)
                .args(["list", "--all", "--json"])
                .output()
        )
        .await
        .map_err(|_| anyhow::anyhow!("Timeout listing containers"))??;

        if !output.status.success() {
            tracing::warn!(
                "Failed to list containers: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON output from exo (warnings now go to stderr in exo)
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

    /// Ensure a Docker image is imported into exo
    ///
    /// This function checks if the image exists in exo, and if not,
    /// imports it from Docker (if available) or returns an error.
    async fn ensure_image_imported(&self, image: &str) -> Result<()> {
        // Check if image already exists in exo
        let images_output = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new(&self.exo_path)
                .args(["images"])
                .output()
        ).await
        .map_err(|_| anyhow::anyhow!("Timeout checking exo images"))?
        .map_err(|e| anyhow::anyhow!("Failed to check exo images: {}", e))?;

        if images_output.status.success() {
            let images_stdout = String::from_utf8_lossy(&images_output.stdout);
            // Check if image name appears in the output
            let base_name = image.split(':').next().unwrap_or(image);
            if images_stdout.contains(base_name) {
                tracing::info!("Image '{}' already exists in exo", image);
                return Ok(());
            }
        }

        tracing::info!("Image '{}' not found in exo, attempting to import from Docker", image);

        // Clone image to own it for the spawned task
        let image_owned = image.to_string();

        // Check if Docker has the image
        let docker_check = StdCommand::new("docker")
            .args(["images", "-q", &image_owned])
            .output();

        match docker_check {
            Ok(output) if !output.stdout.is_empty() => {
                // Image exists in Docker, save and import it
                tracing::info!("Found image in Docker, importing into exo...");

                let temp_file = format!("/tmp/exo_import_{}.tar", std::process::id());
                let temp_file_clone = temp_file.clone();

                // Save image from Docker
                let save_result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(120), // Docker save can take a while
                    tokio::task::spawn_blocking(move || {
                        StdCommand::new("docker")
                            .args(["save", "-o", &temp_file_clone, &image_owned])
                            .output()
                    })
                ).await
                .map_err(|_| anyhow::anyhow!("Timeout saving Docker image"))?
                .map_err(|e| anyhow::anyhow!("Failed to spawn docker save: {}", e))?
                .map_err(|e| anyhow::anyhow!("Failed to save Docker image: {}", e))?;

                if !save_result.status.success() {
                    let stderr = String::from_utf8_lossy(&save_result.stderr);
                    anyhow::bail!("Failed to save Docker image: {}", stderr);
                }

                // Import image into exo
                let import_result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(120), // Import can also take a while
                    Command::new(&self.exo_path)
                        .args(["import", &temp_file])
                        .output()
                ).await
                .map_err(|_| anyhow::anyhow!("Timeout importing image into exo"))?
                .map_err(|e| anyhow::anyhow!("Failed to import image: {}", e))?;

                // Clean up temp file
                let _ = std::fs::remove_file(&temp_file);

                if !import_result.status.success() {
                    let stderr = String::from_utf8_lossy(&import_result.stderr);
                    anyhow::bail!("Failed to import image into exo: {}", stderr);
                }

                tracing::info!("Successfully imported image '{}' from Docker into exo", image);
                Ok(())
            }
            Ok(_) => {
                anyhow::bail!(
                    "Image '{}' not found in Docker. Please pull it first: docker pull {}",
                    image, image
                )
            }
            Err(e) => {
                tracing::warn!("Docker not available: {}", e);
                anyhow::bail!(
                    "Image '{}' not found in exo and Docker is not available. \
                    Please either:\n  1. Import the image manually: exo import <image.tar>\n  2. Pull in Docker: docker pull {}",
                    image, image
                )
            }
        }
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

        // Select image based on agent runtime (before building args so we can import it)
        let default_runtime = AgentRuntime::default();
        let agent_runtime = config.agent_runtime.as_ref().unwrap_or(&default_runtime);
        let image = match agent_runtime {
            AgentRuntime::Openclaw => "openclaw-agent:latest",
            AgentRuntime::ExoNative => "exo-agent:latest",
        };

        // Ensure image is imported into exo before creating container
        self.ensure_image_imported(image).await
            .map_err(|e| anyhow::anyhow!("Failed to import image '{}': {}", image, e))?;

        // Build args for exo run
        let mut args = vec![
            "run".to_string(),
            "-n".to_string(), // Use -n instead of --name
            name.to_string(),
            "--detach".to_string(), // Use --detach instead of -d to avoid conflict with --debug
            "--workdir".to_string(),
            "/agent".to_string(), // Set working directory to /agent
        ];

        // Note: Memory and CPU limits not supported by exo runtime
        // These are validated but not passed to exo CLI
        // args.push("-m".to_string());
        // args.push(format!("{}M", config.memory_mb));
        // args.push("--cpus".to_string());
        // args.push(format!("{}", config.cpu_cores));

        // Use host network mode so container can bind to host ports
        args.push("--network".to_string());
        args.push("host".to_string());

        // Add environment variables
        for (key, value) in self.build_env_vars(config) {
            args.push("-e".to_string());
            // Quote the value if it contains special characters that would be interpreted by the shell
            if value.contains(' ') || value.contains('*') || value.contains('[') || value.contains(']') || value.contains('"') || value.contains('\'') {
                args.push(format!("{}='{}'", key, value));
            } else {
                args.push(format!("{}={}", key, value));
            }
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

        // Get gateway port from config (used in env vars, not port mapping since we use host network)
        let gateway_port: u16 = config
            .env_vars
            .get("PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(18790);

        // Note: With --network host, no port mapping needed - container binds directly to host
        // The PORT env var tells the gateway which port to use

        // Add command to args (image already selected above for import)
        let command = "/entrypoint.sh";
        args.push(image.to_string());
        args.push(command.to_string());

        // Log the full command for debugging
        tracing::info!("Running exo command: {} {}", &self.exo_path, args.join(" "));

        // Use std::process::Command and wait for output
        // Exo outputs to stderr, and we need to wait for the process to complete
        // Wrap in timeout + spawn_blocking to avoid blocking the async runtime for too long

        // Clone values so the closure owns them (required for spawn_blocking)
        let exo_path = self.exo_path.clone();
        let args_clone = args.clone();

        // Create temp files for stdout/stderr to avoid blocking on pipes
        // When stdout is piped, exo's --detach mode hangs because it checks if stdout is a TTY
        let stdout_path = format!("/tmp/exo_out_{}.txt", std::process::id());
        let stderr_path = format!("/tmp/exo_err_{}.txt", std::process::id());

        // Clone paths for use after closure completes
        let stdout_path_read = stdout_path.clone();
        let stderr_path_read = stderr_path.clone();

        let output_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(30), // Should complete quickly with temp files
            tokio::task::spawn_blocking(move || {
                use std::fs::OpenOptions;
                let mut cmd = StdCommand::new(&exo_path);
                cmd.args(&args_clone);

                // Redirect to temp files to avoid pipe blocking issue
                let stdout_file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&stdout_path)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                let stderr_file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&stderr_path)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

                cmd.stdout(stdout_file)
                    .stderr(stderr_file);

                cmd.status()
            })
        ).await;

        // Unwrap the triple-nested Result:
        // Result<Result<Result<ExitStatus, io::Error>, JoinError>, Elapsed>
        let exit_status = match output_result {
            Ok(timeout_result) => match timeout_result {
                Ok(join_result) => match join_result {
                    Ok(status) => status,
                    Err(e) => return Err(anyhow::anyhow!("Failed to run exo command: {}", e)),
                },
                Err(e) => return Err(anyhow::anyhow!("Task join failed: {}", e)),
            },
            Err(_) => return Err(anyhow::anyhow!("Timeout creating agent (exo run took longer than 180 seconds)")),
        };

        tracing::info!("Exo process exited with status: {}", exit_status);

        // Read from temp files
        let stdout_output = std::fs::read_to_string(&stdout_path_read)
            .map_err(|e| anyhow::anyhow!("Failed to read stdout from temp file: {}", e))?;
        let stderr_output = std::fs::read_to_string(&stderr_path_read)
            .map_err(|e| anyhow::anyhow!("Failed to read stderr from temp file: {}", e))?;

        // Clean up temp files
        let _ = std::fs::remove_file(&stdout_path_read);
        let _ = std::fs::remove_file(&stderr_path_read);

        tracing::info!("Exo stdout ({} bytes): {}", stdout_output.len(), stdout_output);
        tracing::info!("Exo stderr ({} bytes): {}", stderr_output.len(), stderr_output);

        // Parse the container ID from stdout (exo outputs to stdout with "Container running in background:")
        let container_id = stdout_output
            .lines()
            .find(|line| line.contains("Container running in background:"))
            .and_then(|line| line.split("Container running in background: ").nth(1))
            .map(|id| id.trim().to_string())
            .or_else(|| {
                // Fallback: try "Starting container:" pattern
                stdout_output
                    .lines()
                    .find(|line| line.contains("Starting container:"))
                    .and_then(|line| line.split("Starting container: ").nth(1))
                    .map(|id| id.trim().to_string())
            })
            .ok_or_else(|| anyhow::anyhow!("Failed to find container ID in exo run output.\nStdout:\n{}\nStderr:\n{}",
                stdout_output, stderr_output))?;

        let id = container_id;

        tracing::info!("Created container: {} ({})", name, id);

        // Verify the container is actually running by checking exo list
        // Use a shorter timeout for this verification since we already have the ID
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(3),
            self.list_containers_internal()
        )
        .await
        {
            Ok(Ok(containers)) => {
                if !containers.iter().any(|c| c.id == id || c.name == name) {
                    // Container not found, but it might still be starting up
                    // Log a warning but continue since we have the ID
                    tracing::warn!("Container '{}' created with ID {} but not yet in exo list", name, id);
                }
            }
            Ok(Err(e)) => {
                // Error listing containers, but container was created with valid ID
                tracing::warn!("Failed to list containers for verification: {}", e);
            }
            Err(_) => {
                // Timeout checking list, but container was created with valid ID
                tracing::warn!("Timeout verifying container in exo list, but ID was parsed successfully");
            }
        }

        Ok(id)
    }

    async fn start_container_internal(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.exo_path)
            .args(["start", id])
            .output()
            .await?;

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
            .output()
            .await?;

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
            .output()
            .await?;

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
        // Stats collection not yet available in exo CLI
        // TODO: Implement when exo adds 'stats' or 'inspect' command
        // See: https://github.com/exo-express/exo/issues
        //
        // Potential approaches when exo adds stats support:
        // 1. Use `exo stats <id>` if available
        // 2. Use `exo inspect <id>` for detailed container info
        // 3. Parse cgroup metrics from /proc for container processes
        Ok(None)
    }

    async fn container_exists_internal(&self, id: &str) -> Result<bool> {
        let containers = self.list_containers_internal().await?;
        Ok(containers.iter().any(|c| c.id == id || c.name == id))
    }

    pub async fn get_logs(&self, id: &str, tail: usize) -> Result<Vec<LogEntry>> {
        let output = Command::new(&self.exo_path)
            .args(["logs", id, "--tail", &tail.to_string()])
            .output()
            .await?;

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
            let mut child = match Command::new(&exo_path)
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
            use tokio::io::{AsyncBufReadExt, BufReader};
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
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

        // Set gateway password for authentication
        // This allows connections from Electron app without device pairing
        env.insert("OPENCLAW_GATEWAY_PASSWORD".to_string(), "claw".to_string());

        // Bind to all interfaces (0.0.0.0) to allow external connections
        env.insert("BIND".to_string(), "lan".to_string());

        // NOTE: Don't set AGENT_NAME - it triggers exo's restrictive "agent-default" security profile
        // which drops all capabilities and blocks syscalls needed by OpenClaw gateway
        // env.insert("AGENT_NAME".to_string(), "claw-agent".to_string());

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LlmProvider, AgentRuntime};

    #[test]
    fn test_build_env_vars_openai() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string()));
        assert!(client.is_ok(), "Should create client with echo command");

        let client = client.unwrap();
        let mut config = AgentConfig::default();

        config.llm_provider = LlmProvider::OpenAI;
        config.llm_model = Some("gpt-4".to_string());
        config.env_vars = std::collections::HashMap::from([
            ("CUSTOM_VAR".to_string(), "custom_value".to_string()),
        ]);

        let env = client.build_env_vars(&config);

        assert_eq!(env.get("LLM_PROVIDER"), Some(&"openai".to_string()));
        assert_eq!(env.get("LLM_MODEL"), Some(&"gpt-4".to_string()));
        assert_eq!(env.get("CUSTOM_VAR"), Some(&"custom_value".to_string()));
        assert!(env.get("AGENT_NAME").is_none(), "AGENT_NAME should not be set");
    }

    #[test]
    fn test_build_env_vars_anthropic() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let mut config = AgentConfig::default();

        config.llm_provider = LlmProvider::Anthropic;
        config.llm_model = Some("claude-3-5-sonnet-20241022".to_string());

        let env = client.build_env_vars(&config);

        assert_eq!(env.get("LLM_PROVIDER"), Some(&"anthropic".to_string()));
        assert_eq!(env.get("LLM_MODEL"), Some(&"claude-3-5-sonnet-20241022".to_string()));
    }

    #[test]
    fn test_build_env_vars_ollama() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let mut config = AgentConfig::default();

        config.llm_provider = LlmProvider::Ollama;
        config.llm_model = Some("llama2".to_string());

        let env = client.build_env_vars(&config);

        assert_eq!(env.get("LLM_PROVIDER"), Some(&"ollama".to_string()));
        assert_eq!(
            env.get("OLLAMA_HOST"),
            Some(&"http://host.containers.internal:11434".to_string())
        );
    }

    #[test]
    fn test_build_env_vars_custom_endpoint() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let mut config = AgentConfig::default();

        config.llm_provider = LlmProvider::Custom {
            endpoint: "https://api.example.com/v1".to_string(),
        };

        let env = client.build_env_vars(&config);

        assert_eq!(env.get("LLM_PROVIDER"), Some(&"custom".to_string()));
        assert_eq!(
            env.get("LLM_ENDPOINT"),
            Some(&"https://api.example.com/v1".to_string())
        );
    }

    #[test]
    fn test_build_env_vars_with_secrets() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let mut config = AgentConfig::default();

        config.secrets = vec!["API_KEY".to_string(), "DATABASE_URL".to_string()];
        config.env_vars = std::collections::HashMap::new();

        let env = client.build_env_vars(&config);

        assert_eq!(env.get("HAS_SECRET_API_KEY"), Some(&"true".to_string()));
        assert_eq!(env.get("HAS_SECRET_DATABASE_URL"), Some(&"true".to_string()));
    }

    #[test]
    fn test_build_mounts_valid() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let _config = AgentConfig::default();

        let volumes = vec![
            VolumeMount {
                source: "/data/claw-pen/volumes/mydata".to_string(),
                target: "/data".to_string(),
                read_only: false,
            },
        ];

        let mounts = client.build_mounts(&volumes);

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0], "/data/claw-pen/volumes/mydata:/data");
    }

    #[test]
    fn test_build_mounts_readonly() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let _config = AgentConfig::default();

        let volumes = vec![VolumeMount {
            source: "/data/claw-pen/volumes/config".to_string(),
            target: "/etc/config".to_string(),
            read_only: true,
        }];

        let mounts = client.build_mounts(&volumes);

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0], "/data/claw-pen/volumes/config:/etc/config:ro");
    }

    #[test]
    fn test_build_mounts_rejects_path_traversal() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let _config = AgentConfig::default();

        let volumes = vec![VolumeMount {
            source: "/data/claw-pen/volumes/../../../etc/passwd".to_string(),
            target: "/data".to_string(),
            read_only: false,
        }];

        let mounts = client.build_mounts(&volumes);

        assert_eq!(mounts.len(), 0, "Path traversal should be rejected");
    }

    #[test]
    fn test_build_mounts_rejects_suspicious_paths() {
        let client = ExoRuntimeClient::new(Some("/bin/echo".to_string())).unwrap();
        let _config = AgentConfig::default();

        let volumes = vec![VolumeMount {
            source: "/var/run/docker.sock".to_string(),
            target: "/docker.sock".to_string(),
            read_only: false,
        }];

        let mounts = client.build_mounts(&volumes);

        assert_eq!(mounts.len(), 0, "Suspicious paths should be rejected");
    }

    #[test]
    fn test_exo_runtime_client_creation_fails_without_binary() {
        let result = ExoRuntimeClient::new(Some("/nonexistent/path/to/exo".to_string()));
        assert!(result.is_err(), "Should fail when exo binary not found");
    }

    #[tokio::test]
    async fn test_exo_runtime_client_with_echo_command() {
        // This test uses /bin/echo which should exist on most systems
        let result = ExoRuntimeClient::new(Some("/bin/echo".to_string()));
        assert!(result.is_ok(), "Should create client with /bin/echo");

        let client = result.unwrap();
        assert_eq!(
            client.exo_path(),
            "/bin/echo",
            "Should store the provided path"
        );
    }

    #[test]
    fn test_default_runtime_for_openclaw() {
        let _config = AgentConfig::default();
        let default_runtime = AgentRuntime::default();

        // Openclaw should be the default
        assert!(matches!(default_runtime, AgentRuntime::Openclaw));
    }
}
