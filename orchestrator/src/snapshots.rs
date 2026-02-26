// Snapshot management - export/import agent state

use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

use crate::types::SnapshotInfo;

pub struct SnapshotManager {
    base_path: PathBuf,
}

impl SnapshotManager {
    pub fn new() -> Result<Self> {
        let base_path = PathBuf::from("/var/lib/claw-pen/snapshots");
        std::fs::create_dir_all(&base_path)?;
        
        Ok(Self { base_path })
    }

    pub fn agent_path(&self, agent_id: &str) -> PathBuf {
        self.base_path.join(agent_id)
    }

    pub fn snapshot_path(&self, agent_id: &str, snapshot_id: &str) -> PathBuf {
        self.agent_path(agent_id).join(snapshot_id)
    }

    pub async fn list_snapshots(&self, agent_id: &str) -> Result<Vec<SnapshotInfo>> {
        let agent_dir = self.agent_path(agent_id);
        let mut snapshots = Vec::new();

        if !agent_dir.exists() {
            return Ok(snapshots);
        }

        for entry in std::fs::read_dir(agent_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let snapshot_id = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Read metadata
                let meta_path = path.join("metadata.json");
                let created_at = if meta_path.exists() {
                    std::fs::read_to_string(&meta_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v["created_at"].as_str().map(|s| s.to_string()))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                // Calculate size
                let size_bytes = self.dir_size(&path).unwrap_or(0);

                snapshots.push(SnapshotInfo {
                    id: snapshot_id,
                    agent_id: agent_id.to_string(),
                    created_at,
                    size_bytes,
                });
            }
        }

        // Sort by created_at descending
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(snapshots)
    }

    pub async fn create_snapshot(&self, agent_id: &str) -> Result<SnapshotInfo> {
        let snapshot_id = Uuid::new_v4().to_string();
        let snapshot_dir = self.snapshot_path(agent_id, &snapshot_id);
        std::fs::create_dir_all(&snapshot_dir)?;

        let created_at = chrono::Utc::now().to_rfc3339();

        // Write metadata
        let metadata = serde_json::json!({
            "id": snapshot_id,
            "agent_id": agent_id,
            "created_at": &created_at,
        });
        std::fs::write(snapshot_dir.join("metadata.json"), metadata.to_string())?;

        // Copy workspace (if exists)
        let workspace_src = PathBuf::from(format!("/var/lib/openclaw/containers/{}/workspace", agent_id));
        if workspace_src.exists() {
            let workspace_dst = snapshot_dir.join("workspace");
            self.copy_dir(&workspace_src, &workspace_dst)?;
        }

        let size_bytes = self.dir_size(&snapshot_dir).unwrap_or(0);

        tracing::info!("Created snapshot {} for agent {}", snapshot_id, agent_id);

        Ok(SnapshotInfo {
            id: snapshot_id,
            agent_id: agent_id.to_string(),
            created_at,
            size_bytes,
        })
    }

    pub async fn restore_snapshot(&self, agent_id: &str, snapshot_id: &str) -> Result<()> {
        let snapshot_dir = self.snapshot_path(agent_id, &snapshot_id);
        
        if !snapshot_dir.exists() {
            anyhow::bail!("Snapshot {} not found for agent {}", snapshot_id, agent_id);
        }

        let workspace_src = snapshot_dir.join("workspace");
        let workspace_dst = PathBuf::from(format!("/var/lib/openclaw/containers/{}/workspace", agent_id));

        if workspace_src.exists() {
            // Remove existing workspace
            if workspace_dst.exists() {
                std::fs::remove_dir_all(&workspace_dst)?;
            }
            
            // Restore from snapshot
            self.copy_dir(&workspace_src, &workspace_dst)?;
        }

        tracing::info!("Restored snapshot {} for agent {}", snapshot_id, agent_id);
        Ok(())
    }

    pub async fn delete_snapshot(&self, agent_id: &str, snapshot_id: &str) -> Result<()> {
        let snapshot_dir = self.snapshot_path(agent_id, &snapshot_id);
        
        if snapshot_dir.exists() {
            std::fs::remove_dir_all(&snapshot_dir)?;
            tracing::info!("Deleted snapshot {} for agent {}", snapshot_id, agent_id);
        }

        Ok(())
    }

    /// Export agent config as JSON (for backup/migration)
    pub async fn export_agent(&self, agent_id: &str) -> Result<String> {
        // This would be called with the full agent container from state
        // For now, just return a placeholder
        // The actual export is done in the API handler with full agent data
        Ok(String::new())
    }

    fn copy_dir(&self, src: &PathBuf, dst: &PathBuf) -> Result<()> {
        if !dst.exists() {
            std::fs::create_dir_all(dst)?;
        }

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if ty.is_dir() {
                self.copy_dir(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }

    fn dir_size(&self, path: &PathBuf) -> Result<u64> {
        let mut size = 0;

        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let ty = entry.file_type()?;

                if ty.is_dir() {
                    size += self.dir_size(&entry.path())?;
                } else {
                    size += entry.metadata()?.len();
                }
            }
        }

        Ok(size)
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new().expect("Failed to create SnapshotManager")
    }
}
