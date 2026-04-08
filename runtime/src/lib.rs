//! Claw Pen Runtime - Exo container runtime integration
//!
//! This module provides the runtime for executing Claw Pen agents in exo containers.

/// Default agent image for Claw Pen agents
pub const DEFAULT_AGENT_IMAGE: &str = "node:20-alpine";

/// Default gateway port for agent communication
pub const DEFAULT_GATEWAY_PORT: u16 = 18790;

pub mod agent;
pub mod daemon;
pub mod config;

pub use agent::{AgentContainer, AgentRuntime, ContainerStatus, AgentSpec};
pub use daemon::{ExoDaemonClient, DaemonRequest, DaemonResponse};
pub use config::RuntimeConfig;

use anyhow::Result;

/// Create a new runtime instance
pub fn runtime() -> Result<Box<dyn AgentRuntime>> {
    let config = RuntimeConfig::from_env()?;
    Ok(Box::new(ExoDaemonClient::new(config)?))
}

/// Initialize the runtime (ensure daemon is running, etc.)
pub async fn initialize() -> Result<()> {
    let config = RuntimeConfig::from_env()?;
    let client = ExoDaemonClient::new(config)?;
    client.ensure_daemon_running().await
}
