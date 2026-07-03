use std::path::{Path, PathBuf};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use serde::{Serialize, Deserialize};
use chrono::Utc;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
 pub session_id: String,
 pub created_at: String,
 pub last_active: String,
 pub current_path: String, 
}

pub struct SessionManager {
 pub base_dir: PathBuf,
}

impl SessionManager {
 pub fn new(base_dir: PathBuf) -> Self {
 Self { base_dir }
 }

 pub async fn create_session(&self, session_id: &str, path_name: &str) -> Result<PathBuf> {
 let session_path = self.base_dir.join(session_id);
 let workspace_path = session_path.join("workspace");
 let history_file = session_path.join("history.jsonl");

 fs::create_dir_all(&workspace_path).await
.with_context(|| format!("Failed to create workspace for {}", session_id))?;

 if !history_file.exists() {
 fs::File::create(&history_file).await?;
 }

 let metadata = SessionMetadata {
 session_id: session_id.to_string(),
 created_at: Utc::now().to_rfc3339(),
 last_active: Utc::now().to_rfc3339(),
 current_path: path_name.to_string(),
 };
 
 let meta_file = session_path.join("metadata.json");
 let meta_json = serde_json::to_string_pretty(&metadata)?;
 fs::write(meta_file, meta_json).await?;

 Ok(session_path)
 }

 pub async fn append_history(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
 let history_file = self.base_dir.join(session_id).join("history.jsonl");
 
 let entry = serde_json::json!({
 "role": role,
 "content": content,
 "timestamp": Utc::now().to_rfc3339()
 });

 let mut file = OpenOptions::new()
.append(true)
.open(&history_file)
.await?;

 let mut line = serde_json::to_string(&entry)?;
 line.push('\n');
 file.write_all(line.as_bytes()).await?;
 
 Ok(())
 }

 pub async fn get_recent_history(&self, session_id: &str, limit: usize) -> Result<Vec<serde_json::Value>> {
 let history_file = self.base_dir.join(session_id).join("history.jsonl");
 let contents = fs::read_to_string(&history_file).await?;
 
 let lines: Vec<&str> = contents.lines().collect();
 let start = lines.len().saturating_sub(limit);
 
 let mut history = Vec::new();
 for line in &lines[start..] {
 if let Ok(val) = serde_json::from_str(line) {
 history.push(val);
 }
 }

 Ok(history)
 }

 pub fn get_workspace_path(&self, session_id: &str) -> PathBuf {
 self.base_dir.join(session_id).join("workspace")
 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_lifecycle() -> anyhow::Result<()> {
        let tmp_dir = tempdir()?;
        let base_dir = tmp_dir.path().to_path_buf();
        let sm = SessionManager::new(base_dir.clone());

        let sid = "test_sid";
        sm.create_session(sid, "test").await?;
        sm.append_history(sid, "user", "hi").await?;
        let hist = sm.get_recent_history(sid, 1).await?;
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0]["content"], "hi");

        Ok(())
    }
}
