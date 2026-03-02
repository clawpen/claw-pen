use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const AGENTS_FILE: &str = "agents.json";

/// Get the data directory for storing agent configurations
fn get_data_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .map(|d| d.join("claw-pen"))
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Storage format for agents on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub config: crate::types::AgentConfig,
    pub created_at: String,
    pub updated_at: String,
    /// Container runtime: "docker" or "exo"
    #[serde(default)]
    pub runtime: Option<String>,
}

/// Load all persisted agents from disk
pub fn load_agents() -> Result<Vec<StoredAgent>> {
    let data_dir = get_data_dir()?;
    let agents_file = data_dir.join(AGENTS_FILE);

    if !agents_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(agents_file)?;
    let agents: Vec<StoredAgent> = serde_json::from_str(&content)?;
    Ok(agents)
}

/// Save all agents to disk
pub fn save_agents(agents: &[StoredAgent]) -> Result<()> {
    let data_dir = get_data_dir()?;
    let agents_file = data_dir.join(AGENTS_FILE);

    let content = serde_json::to_string_pretty(agents)?;
    fs::write(agents_file, content)?;
    Ok(())
}

/// Add or update an agent in storage
pub fn upsert_agent(agent: &StoredAgent) -> Result<()> {
    let mut agents = load_agents()?;

    // Update or add
    if let Some(existing) = agents.iter_mut().find(|a| a.id == agent.id) {
        *existing = agent.clone();
    } else {
        agents.push(agent.clone());
    }

    save_agents(&agents)?;
    Ok(())
}

/// Remove an agent from storage
pub fn remove_agent(id: &str) -> Result<()> {
    let mut agents = load_agents()?;
    agents.retain(|a| a.id != id);
    save_agents(&agents)?;
    Ok(())
}

/// Convert StoredAgent to AgentContainer (for API responses)
impl From<StoredAgent> for crate::types::AgentContainer {
    fn from(stored: StoredAgent) -> Self {
        use crate::types::AgentStatus;
        let status = match stored.status.as_str() {
            "running" => AgentStatus::Running,
            "stopped" => AgentStatus::Stopped,
            "starting" => AgentStatus::Starting,
            "stopping" => AgentStatus::Stopping,
            "error" => AgentStatus::Error,
            _ => AgentStatus::Stopped,
        };

        Self {
            id: stored.id,
            name: stored.name,
            status,
            config: stored.config,
            tailscale_ip: None,
            resource_usage: None,
            project: None,
            tags: vec![],
            restart_policy: Default::default(),
            health_status: None,
            runtime: stored.runtime,
        }
    }
}

/// Convert AgentContainer to StoredAgent (for persistence)
pub fn to_stored_agent(container: &crate::types::AgentContainer) -> StoredAgent {
    let now = chrono();
    StoredAgent {
        id: container.id.clone(),
        name: container.name.clone(),
        status: format!("{:?}", container.status),
        config: container.config.clone(),
        created_at: now.clone(), // In production, track original creation time
        updated_at: now,
        runtime: container.runtime.clone(),
    }
}

/// Simple current time for timestamps
fn chrono() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", duration)
}
