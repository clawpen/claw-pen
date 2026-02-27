// Secrets management - file-based secure storage

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::io::Write;

use crate::types::SecretInfo;

pub struct SecretsManager {
    base_path: PathBuf,
}

impl SecretsManager {
    pub fn new() -> Result<Self> {
        let base_path = PathBuf::from("/var/lib/claw-pen/secrets");
        std::fs::create_dir_all(&base_path)?;

        Ok(Self { base_path })
    }

    pub fn agent_path(&self, agent_id: &str) -> PathBuf {
        self.base_path.join(agent_id)
    }

    pub async fn list_secrets(&self, agent_id: &str) -> Result<Vec<SecretInfo>> {
        let agent_dir = self.agent_path(agent_id);
        let mut secrets = Vec::new();

        if !agent_dir.exists() {
            return Ok(secrets);
        }

        for entry in std::fs::read_dir(agent_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let metadata = entry.metadata()?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let created_at = metadata
                    .created()
                    .ok()
                    .and_then(|t| {
                        use std::time::UNIX_EPOCH;
                        t.duration_since(UNIX_EPOCH).ok()
                    })
                    .map(|d| {
                        chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                secrets.push(SecretInfo {
                    name,
                    created_at,
                    size_bytes: metadata.len(),
                });
            }
        }

        Ok(secrets)
    }

    pub async fn set_secret(&self, agent_id: &str, name: &str, value: &str) -> Result<()> {
        let agent_dir = self.agent_path(agent_id);
        std::fs::create_dir_all(&agent_dir)?;

        let secret_path = agent_dir.join(name);

        // Write with restricted permissions (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&secret_path)?
                .write_all(value.as_bytes())?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&secret_path, value)?;
        }

        tracing::info!("Set secret '{}' for agent {}", name, agent_id);
        Ok(())
    }

    pub async fn delete_secret(&self, agent_id: &str, name: &str) -> Result<()> {
        let secret_path = self.agent_path(agent_id).join(name);

        if secret_path.exists() {
            std::fs::remove_file(&secret_path)?;
            tracing::info!("Deleted secret '{}' for agent {}", name, agent_id);
        }

        Ok(())
    }

    pub async fn get_secret(&self, agent_id: &str, name: &str) -> Result<Option<String>> {
        let secret_path = self.agent_path(agent_id).join(name);

        if secret_path.exists() {
            let value = std::fs::read_to_string(&secret_path)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Get all secrets for an agent as a map
    pub async fn get_all_secrets(&self, agent_id: &str) -> Result<HashMap<String, String>> {
        let infos = self.list_secrets(agent_id).await?;
        let mut secrets = HashMap::new();

        for info in infos {
            if let Some(value) = self.get_secret(agent_id, &info.name).await? {
                secrets.insert(info.name, value);
            }
        }

        Ok(secrets)
    }

    /// Get mount path for secrets (used by container runtime)
    pub fn mount_path(&self) -> PathBuf {
        PathBuf::from("/run/secrets")
    }
}

impl Default for SecretsManager {
    fn default() -> Self {
        Self::new().expect("Failed to create SecretsManager")
    }
}
