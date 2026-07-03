use crate::session_manager::Session;
use std::path::PathBuf;

pub struct BriefingEngine {
    pub max_history_messages: usize,
}

impl BriefingEngine {
    pub fn new(max_history_messages: usize) -> Self {
        Self {
            max_history_messages,
        }
    }

    /// Assembles a briefing prompt based on the current session state.
    /// This is what tells the "visiting" agent who they are and what happened before.
    pub fn assemble_briefing(&self, session: &Session) -> String {
        let mut briefing = String::new();

        // 1. Identity & Purpose
        briefing.push_str("### SESSION CONTEXT\n");
        briefing.push_str(&format!("Session ID: {}\n", session.id));
        briefing.push_str(&format!("Workspace Path: {:?}\n\n", session.workspace_path));

        // 2. History Summary / Recent Context
        briefing.push_str("### RECENT HISTORY\n");
        let history = session.get_recent_history(self.max_history_messages);
        
        if history.is_empty() {
            briefing.push_str("No prior conversation history in this session.\n");
        } else {
            for msg in history {
                let role = match msg.role.as_str() {
                    "user" => "User",
                    "assistant" => "Assistant",
                    _ => "System",
                };
                briefing.push_str(&format!("{}: {}\n", role, msg.content));
            }
        }

        briefing.push_str("\n### INSTRUCTIONS\n");
        briefing.push_str("You are a specialist agent visiting this session to perform a specific task. \n");
        briefing.push_str("Review the history above to understand the current state and objectives. \n");
        briefing.push_str("Maintain continuity with the previous interactions.\n");

        briefing
    }
}
