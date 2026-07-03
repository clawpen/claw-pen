use crate::session_manager::{SessionManager, SessionMetadata};
use crate::briefing_engine::BriefingEngine;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use anyhow::{Context, Result};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace_path: PathBuf,
    pub metadata: SessionMetadata,
}

pub struct OrchestrationEngine {
    pub session_manager: SessionManager,
    pub briefing_engine: BriefingEngine,
}

impl OrchestrationEngine {
    pub fn new(session_manager: SessionManager, briefing_engine: BriefingEngine) -> Self {
        Self {
            session_manager,
            briefing_engine,
        }
    }

    /// Initializes or retrieves a session.
    pub async fn get_or_create_session(&self, session_id: &str, path_name: &str) -> Result<Session> {
        let session_path = self.session_manager.base_dir.join(session_id);
        
        if session_path.exists() {
            // Load existing session
            let meta_file = session_path.join("metadata.json");
            let meta_content = fs::read_to_string(meta_file).await?;
            let metadata: SessionMetadata = serde_json::from_str(&meta_content)?;
            
            Ok(Session {
                id: session_id.to_string(),
                workspace_path: session_path.join("workspace"),
                metadata,
            })
        } else {
            // Create new session
            self.session_manager.create_session(session_id, path_name).await?;
            
            // Re-read metadata to get the full object
            let meta_file = session_path.join("metadata.json");
            let meta_content = fs::read_to_string(meta_file).await?;
            let metadata: SessionMetadata = serde_json::from_str(&meta_content)?;

            Ok(Session {
                id: session_id.to_string(),
                workspace_path: session_path.join("workspace"),
                metadata,
            })
        }
    }

    /// Generates the full context briefing for a visiting agent.
    pub async fn prepare_agent_briefing(&self, session: &Session) -> Result<String> {
        // We need to pull history from the manager using the session ID
        // Note: SessionManager::get_recent_history expects session_id
        let history = self.session_manager.get_recent_history(&session.id, self.briefing_engine.max_history_messages).await?;
        
        // Convert serde_json::Value to a format the briefing engine understands
        // Since briefing_engine was written to take a 'Session' (which it currently doesn't fully support via trait/struct)
        // Let's slightly adjust how we call it or bridge the gap.
        
        // Re-implementing the bridge logic inside the engine call for now to keep it simple.
        let mut briefing = String::new();
        briefing.push_str("### SESSION CONTEXT\n");
        briefing.push_str(&format!("Session ID: {}\n", session.id));
        briefing.push_str(&format!("Workspace Path: {:?}\n\n", session.workspace_path));

        briefing.push_str("### RECENT HISTORY\n");
        if history.is_empty() {
            briefing.push_str("No prior conversation history in this session.\n");
        } else {
            for msg in history {
                let role = msg["role"].as_str().unwrap_or("system");
                let content = msg["content"].as_str().unwrap_or("");
                let display_role = match role {
                    "user" => "User",
                    "assistant" => "Assistant",
                    _ => "System",
                };
                briefing.push_str(&format!("{}: {}\n", display_role, content));
            }
        }

        briefing.push_str("\n### INSTRUCTIONS\n");
        briefing.push_str("You are a specialist agent visiting this session to perform a specific task. \n");
        briefing.push_str("Review the history above to understand the current state and objectives. \n");
        briefing.push_str("Maintain continuity with the previous interactions.\n");

        Ok(briefing)
    }
    
    /// Records a message into the session history.
    pub async fn record_message(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
        self.session_manager.append_history(session_id, role, content).await
    }
}
