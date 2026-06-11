//! Minimal team registry for user role management.
//! Phase One: stripped of all agent routing logic.

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;

pub struct TeamRegistry {
    teams: RwLock<HashMap<String, serde_json::Value>>,
}

impl TeamRegistry {
    pub fn new(_data_dir: &Path) -> Result<Self> {
        // Hardcoded minimal teams for classroom use
        let mut teams = HashMap::new();
        
        teams.insert(
            "default".to_string(),
            json!({
                "id": "default",
                "name": "Default Class",
                "description": "Default classroom group",
                "roles": ["student", "teacher", "admin"]
            }),
        );
        
        teams.insert(
            "math-101".to_string(),
            json!({
                "id": "math-101",
                "name": "Math 101",
                "description": "Introduction to Mathematics",
                "roles": ["student", "teacher", "admin"]
            }),
        );
        
        teams.insert(
            "cs-101".to_string(),
            json!({
                "id": "cs-101",
                "name": "CS 101",
                "description": "Introduction to Computer Science",
                "roles": ["student", "teacher", "admin"]
            }),
        );

        Ok(Self {
            teams: RwLock::new(teams),
        })
    }

    pub fn list_teams(&self) -> Vec<serde_json::Value> {
        let teams = self.teams.blocking_read();
        teams.values().cloned().collect()
    }

    pub fn list_roles(&self, _team_id: &str) -> Vec<serde_json::Value> {
        vec![
            json!({"id": "student", "name": "Student", "permissions": ["chat", "read"]}),
            json!({"id": "teacher", "name": "Teacher", "permissions": ["chat", "read", "manage_users", "approve"]}),
            json!({"id": "admin", "name": "Admin", "permissions": ["chat", "read", "manage_users", "approve", "delete"]}),
        ]
    }
}
