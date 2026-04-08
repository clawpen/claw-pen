//! Native inference service management
//!
//! This module manages the built-in GGUF inference service,
//! allowing Claw Pen to run local LLMs without external dependencies.

use anyhow::Result;
use std::path::Path;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::NativeInferenceConfig;

/// Manager for the native inference service
pub struct InferenceManager {
    config: NativeInferenceConfig,
    process: RwLock<Option<InferenceProcess>>,
}

struct InferenceProcess {
    pid: u32,
}

impl InferenceManager {
    /// Create a new inference manager
    pub fn new(config: NativeInferenceConfig) -> Self {
        Self {
            config,
            process: RwLock::new(None),
        }
    }

    /// Start the inference service
    pub async fn start(&self) -> Result<()> {
        // Check if already running
        {
            let proc = self.process.read().await;
            if proc.is_some() {
                info!("Native inference service already running");
                return Ok(());
            }
        }

        // Check if model file exists
        if !Path::new(&self.config.model_path).exists() {
            return Err(anyhow::anyhow!(
                "Model file not found: {}",
                self.config.model_path
            ));
        }

        info!("Starting native inference service with model: {}", self.config.model_path);

        // Build the inference service binary path
        let bin_path = self.find_inference_binary()?;

        // Spawn the inference service process
        let model_path = self.config.model_path.clone();
        let port = self.config.port;

        let child = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&bin_path)
                .arg("--model-path")
                .arg(&model_path)
                .arg("--port")
                .arg(port.to_string())
                .spawn()
        })
        .await??;

        let pid = child.id();
        info!("Inference service started with PID: {}", pid);

        // Store the process handle
        *self.process.write().await = Some(InferenceProcess { pid });

        // Give the service a moment to start
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Health check
        self.health_check().await?;

        Ok(())
    }

    /// Stop the inference service
    pub async fn stop(&self) -> Result<()> {
        let mut proc = self.process.write().await;
        if let Some(inf_proc) = proc.take() {
            info!("Stopping inference service (PID: {})", inf_proc.pid);

            // Kill the process
            let result = tokio::task::spawn_blocking(move || {
                kill_process(inf_proc.pid)
            }).await;

            if let Err(e) = result {
                warn!("Error stopping inference service: {}", e);
            }
        }
        Ok(())
    }

    /// Check if the service is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("http://localhost:{}/v1/models", self.config.port);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        let response = client.get(&url).send().await?;

        Ok(response.status() == 200)
    }

    /// Get the endpoint URL for the inference service
    pub fn endpoint(&self) -> String {
        format!("http://localhost:{}", self.config.port)
    }

    /// Get the default model name
    pub fn model_name(&self) -> String {
        // Extract model name from path
        Path::new(&self.config.model_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Find the inference service binary
    fn find_inference_binary(&self) -> Result<std::path::PathBuf> {
        // Try multiple locations for the binary
        let possible_paths = vec![
            // Workspace release build (most likely when building from workspace root)
            "../target/release/claw-pen-inference.exe",
            "../target/release/claw-pen-inference",
            "../target/debug/claw-pen-inference.exe",
            "../target/debug/claw-pen-inference",
            // Development build location
            "../inference/target/debug/claw-pen-inference.exe",
            "../inference/target/debug/claw-pen-inference",
            // Release build location
            "../inference/target/release/claw-pen-inference.exe",
            "../inference/target/release/claw-pn-inference",
            // Installed location
            "claw-pen-inference.exe",
            "claw-pen-inference",
            // Full paths from project root
            "F:/Software/Claw Pen/claw-pen/target/release/claw-pen-inference.exe",
            "F:/Software/Claw Pen/claw-pen/inference/target/debug/claw-pen-inference.exe",
        ];

        for path in &possible_paths {
            let path_buf = std::path::PathBuf::from(path);
            if path_buf.exists() {
                info!("Found inference binary at: {}", path_buf.display());
                return Ok(path_buf);
            }
        }

        Err(anyhow::anyhow!(
            "Could not find inference service binary. Tried: {:?}",
            possible_paths
        ))
    }
}

impl Drop for InferenceManager {
    fn drop(&mut self) {
        // Note: We can't easily stop the process here because we're in a synchronous context
        // and the process handle is behind an RwLock. The process will be terminated
        // when the orchestrator exits, or can be explicitly stopped via the stop() method.
        // For a clean shutdown, the orchestrator should call stop() before dropping.
    }
}

/// Kill a process by PID
#[cfg(windows)]
fn kill_process(pid: u32) -> Result<()> {
    std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output()?;
    Ok(())
}

#[cfg(not(windows))]
fn kill_process(pid: u32) -> Result<()> {
    std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .output()?;
    Ok(())
}
