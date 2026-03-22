//! Team management and routing logic
//!
//! Teams provide a single entry point where a router agent classifies
//! incoming messages and routes them to the appropriate specialist agent.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;

use crate::types::*;

// Default functions for serde
fn default_router_mode() -> RouterMode {
    RouterMode::Hybrid
}

fn default_confidence_threshold() -> f32 {
    0.7
}

fn default_true() -> bool {
    true
}

/// Team configuration loaded from TOML
#[derive(Debug, Clone, Deserialize)]
struct TeamConfig {
    team: TeamMeta,
    router: RouterConfigRaw,
    agents: HashMap<String, TeamAgent>,
    routing: HashMap<String, RoutingRule>,
    #[serde(default)]
    clarification: ClarificationConfig,
    #[serde(default)]
    responses: ResponseTemplates,
}

#[derive(Debug, Clone, Deserialize)]
struct TeamMeta {
    name: String,
    description: Option<String>,
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RouterConfigRaw {
    name: String,
    #[serde(default = "default_router_mode")]
    mode: RouterMode,
    #[serde(default = "default_confidence_threshold")]
    confidence_threshold: f32,
    #[serde(default = "default_true")]
    clarify_on_low_confidence: bool,
}

/// Registry of all teams
pub struct TeamRegistry {
    teams: RwLock<HashMap<String, Team>>,
    teams_dir: String,
    /// Dynamic role assignments: (team_id, intent) -> agent_id
    role_assignments: RwLock<HashMap<(String, String), TeamRoleAssignment>>,
}

impl TeamRegistry {
    pub fn new(teams_dir: &str) -> Self {
        Self {
            teams: RwLock::new(HashMap::new()),
            teams_dir: teams_dir.to_string(),
            role_assignments: RwLock::new(HashMap::new()),
        }
    }

    /// Get a team by ID
    pub async fn get_team(&self, team_id: &str) -> Option<Team> {
        let teams = self.teams.read().await;
        teams.get(team_id).cloned()
    }

    /// Load all teams from the teams directory
    pub async fn load_all(&self) -> Result<usize> {
        let path = Path::new(&self.teams_dir);
        if !path.exists() {
            tracing::warn!("Teams directory does not exist: {}", self.teams_dir);
            return Ok(0);
        }

        let mut loaded = 0;
        let mut teams = self.teams.write().await;

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            // Skip template files
            if file_path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with('_'))
                .unwrap_or(false)
            {
                continue;
            }

            if file_path.extension().map(|e| e == "toml").unwrap_or(false) {
                match self.load_team_config(&file_path) {
                    Ok(team) => {
                        tracing::info!("Loaded team: {} ({})", team.name, team.id);
                        teams.insert(team.id.clone(), team);
                        loaded += 1;
                    }
                    Err(e) => {
                        tracing::error!("Failed to load team from {:?}: {}", file_path, e);
                    }
                }
            }
        }

        Ok(loaded)
    }

    fn load_team_config(&self, path: &Path) -> Result<Team> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read team config: {:?}", path))?;

        let config: TeamConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse team config: {:?}", path))?;

        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(Team {
            id,
            name: config.team.name,
            description: config.team.description,
            version: config.team.version,
            router: RouterConfig {
                name: config.router.name,
                mode: config.router.mode,
                confidence_threshold: config.router.confidence_threshold,
                clarify_on_low_confidence: config.router.clarify_on_low_confidence,
            },
            agents: config.agents,
            routing: config.routing,
            clarification: config.clarification,
            responses: config.responses,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: TeamStatus::Active,
        })
    }

    /// List all teams
    pub async fn list(&self) -> Vec<Team> {
        let teams = self.teams.read().await;
        teams.values().cloned().collect()
    }

    /// Get a team by ID
    pub async fn get(&self, id: &str) -> Option<Team> {
        let teams = self.teams.read().await;
        teams.get(id).cloned()
    }

    /// Add or update a team
    pub async fn upsert(&self, team: Team) {
        let mut teams = self.teams.write().await;
        teams.insert(team.id.clone(), team);
    }

    /// Remove a team
    pub async fn remove(&self, id: &str) -> Option<Team> {
        let mut teams = self.teams.write().await;
        teams.remove(id)
    }

    // ========================================================================
    // DYNAMIC ROLE ASSIGNMENT
    // ========================================================================

    /// Assign an agent to a team role
    pub async fn assign_role(&self, team_id: &str, intent: &str, agent_id: &str, assigned_by: &str) -> Result<TeamRoleAssignment> {
        // Verify team exists
        let teams = self.teams.read().await;
        if !teams.contains_key(team_id) {
            return Err(anyhow::anyhow!("Team not found: {}", team_id));
        }
        drop(teams);

        // Verify intent exists in team
        let teams = self.teams.read().await;
        let team = teams.get(team_id).unwrap();
        if !team.agents.contains_key(intent) {
            return Err(anyhow::anyhow!("Intent '{}' not found in team '{}'", intent, team_id));
        }
        drop(teams);

        // Create assignment
        let assignment = TeamRoleAssignment {
            id: uuid::Uuid::new_v4().to_string(),
            team_id: team_id.to_string(),
            intent: intent.to_string(),
            agent_id: agent_id.to_string(),
            assigned_at: Utc::now().to_rfc3339(),
            assigned_by: assigned_by.to_string(),
        };

        // Store assignment
        let mut assignments = self.role_assignments.write().await;
        assignments.insert((team_id.to_string(), intent.to_string()), assignment.clone());

        Ok(assignment)
    }

    /// Remove an agent from a team role
    pub async fn remove_role(&self, team_id: &str, intent: &str) -> Option<TeamRoleAssignment> {
        let mut assignments = self.role_assignments.write().await;
        assignments.remove(&(team_id.to_string(), intent.to_string()))
    }

    /// Get the agent assigned to a specific role
    pub async fn get_role_assignment(&self, team_id: &str, intent: &str) -> Option<TeamRoleAssignment> {
        let assignments = self.role_assignments.read().await;
        assignments.get(&(team_id.to_string(), intent.to_string())).cloned()
    }

    /// List all role assignments for a team
    pub async fn list_team_assignments(&self, team_id: &str) -> Vec<TeamRoleAssignment> {
        let assignments = self.role_assignments.read().await;
        assignments
            .iter()
            .filter(|((tid, _intent), _assignment)| tid == team_id)
            .map(|(_key, assignment)| assignment.clone())
            .collect()
    }

    /// Get the actual agent ID for a team intent (returns assigned agent or default)
    pub async fn resolve_agent(&self, team_id: &str, intent: &str) -> Option<String> {
        // First check if there's a dynamic assignment
        if let Some(assignment) = self.get_role_assignment(team_id, intent).await {
            return Some(assignment.agent_id);
        }

        // Fall back to default agent from team config
        let teams = self.teams.read().await;
        let team = teams.get(team_id)?;
        let agent_config = team.agents.get(intent)?;
        Some(agent_config.agent.clone())
    }

    /// Get an agent's current role assignment (reverse lookup)
    pub async fn get_agent_role(&self, agent_id: &str) -> Option<(String, String, String)> {
        // Returns (team_id, role_id, role_name)
        let assignments = self.role_assignments.read().await;

        for ((team_id, intent), assignment) in assignments.iter() {
            if assignment.agent_id == agent_id {
                // Get team and role info
                let teams = self.teams.read().await;
                if let Some(team) = teams.get(team_id) {
                    if let Some(role_info) = team.agents.get(intent) {
                        let role_name = role_info.description
                            .split(" - ")
                            .next()
                            .unwrap_or(intent)
                            .to_string();
                        return Some((team_id.clone(), intent.clone(), role_name));
                    }
                }
            }
        }
        None
    }
}

/// Router classifies messages and determines target agent
pub struct Router {
    team: Team,
}

impl Router {
    pub fn new(team: Team) -> Self {
        Self { team }
    }

    /// Classify a message to determine routing
    pub fn classify(&self, message: &str) -> ClassificationResult {
        match self.team.router.mode {
            RouterMode::Keyword => self.classify_by_keywords(message),
            RouterMode::Llm => self.classify_by_llm(message),
            RouterMode::Hybrid => {
                // Try keywords first
                let keyword_result = self.classify_by_keywords(message);

                // If high confidence, use it
                if keyword_result.confidence >= self.team.router.confidence_threshold {
                    keyword_result
                } else {
                    // Fall back to LLM classification
                    let llm_result = self.classify_by_llm(message);

                    // Use whichever has higher confidence
                    if llm_result.confidence > keyword_result.confidence {
                        llm_result
                    } else {
                        keyword_result
                    }
                }
            }
        }
    }

    fn classify_by_keywords(&self, message: &str) -> ClassificationResult {
        let message_lower = message.to_lowercase();
        let mut best_match: Option<(String, f32, Vec<String>)> = None;

        for (intent, rule) in &self.team.routing {
            let mut matched_keywords = Vec::new();
            let mut score = 0.0f32;

            for keyword in &rule.keywords {
                if message_lower.contains(&keyword.to_lowercase()) {
                    matched_keywords.push(keyword.clone());
                    // Score based on keyword length (longer = more specific)
                    score += keyword.len() as f32 / 10.0;
                }
            }

            if !matched_keywords.is_empty() {
                // Normalize score to 0.0-1.0
                let confidence = (score / matched_keywords.len() as f32).min(1.0);

                if best_match.is_none() || confidence > best_match.as_ref().unwrap().1 {
                    best_match = Some((intent.clone(), confidence, matched_keywords));
                }
            }
        }

        match best_match {
            Some((intent, confidence, matched_keywords)) => {
                let needs_clarification = confidence < self.team.router.confidence_threshold
                    && self.team.router.clarify_on_low_confidence;

                ClassificationResult {
                    intent,
                    confidence,
                    matched_keywords,
                    needs_clarification,
                }
            }
            None => ClassificationResult {
                intent: "unknown".to_string(),
                confidence: 0.0,
                matched_keywords: vec![],
                needs_clarification: true,
            },
        }
    }

    fn classify_by_llm(&self, _message: &str) -> ClassificationResult {
        // TODO: Implement LLM-based classification
        // For now, return low-confidence unknown
        tracing::warn!("LLM routing not yet implemented, falling back to keyword");

        ClassificationResult {
            intent: "unknown".to_string(),
            confidence: 0.0,
            matched_keywords: vec![],
            needs_clarification: true,
        }
    }

    /// Get the target agent for a classification result
    pub fn get_target_agent(&self, classification: &ClassificationResult) -> Option<&TeamAgent> {
        self.team.agents.get(&classification.intent)
    }

    /// Generate clarification message
    pub fn generate_clarification(&self) -> String {
        let options: Vec<String> = self
            .team
            .agents
            .iter()
            .map(|(intent, agent)| {
                self.team
                    .clarification
                    .options_format
                    .replace("{intent}", intent)
                    .replace("{description}", &agent.description)
            })
            .collect();

        let prompt = self
            .team
            .clarification
            .prompts
            .first()
            .cloned()
            .unwrap_or_else(|| "What would you like help with?".to_string());

        format!("{}\n\n{}", prompt, options.join("\n"))
    }

    /// Get routing acknowledgment message
    pub fn get_routing_ack(&self, agent_name: &str) -> String {
        self.team
            .responses
            .routing_ack
            .replace("{agent_name}", agent_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_team() -> Team {
        let mut agents = HashMap::new();
        agents.insert(
            "receipts".to_string(),
            TeamAgent {
                agent: "finn".to_string(),
                description: "Handles receipts".to_string(),
            },
        );
        agents.insert(
            "payables".to_string(),
            TeamAgent {
                agent: "pax".to_string(),
                description: "Handles bills".to_string(),
            },
        );

        let mut routing = HashMap::new();
        routing.insert(
            "receipts".to_string(),
            RoutingRule {
                keywords: vec!["receipt".to_string(), "expense".to_string()],
                examples: vec![],
            },
        );
        routing.insert(
            "payables".to_string(),
            RoutingRule {
                keywords: vec!["pay".to_string(), "bill".to_string(), "owe".to_string()],
                examples: vec![],
            },
        );

        Team {
            id: "test-team".to_string(),
            name: "Test Team".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            router: RouterConfig {
                name: "test-router".to_string(),
                mode: RouterMode::Keyword,
                confidence_threshold: 0.7,
                clarify_on_low_confidence: true,
            },
            agents,
            routing,
            clarification: ClarificationConfig::default(),
            responses: ResponseTemplates::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            status: TeamStatus::Active,
        }
    }

    #[test]
    fn test_keyword_routing() {
        let team = create_test_team();
        let router = Router::new(team);

        let result = router.classify("I have a receipt to submit");
        assert_eq!(result.intent, "receipts");
        assert!(!result.needs_clarification);
    }

    #[test]
    fn test_unknown_routing() {
        let team = create_test_team();
        let router = Router::new(team);

        let result = router.classify("Hello there!");
        assert_eq!(result.intent, "unknown");
        assert!(result.needs_clarification);
    }

    #[test]
    fn test_clarification_generation() {
        let team = create_test_team();
        let router = Router::new(team);

        let clarification = router.generate_clarification();
        assert!(clarification.contains("receipts"));
        assert!(clarification.contains("payables"));
    }

    #[test]
    fn test_load_design_build_firm() {
        let registry = TeamRegistry::new("../teams");
        let team = registry.load_team_config(Path::new("../teams/design-build-firm.toml"));

        assert!(team.is_ok());
        let team = team.unwrap();

        assert_eq!(team.name, "Design-Build Firm");
        assert_eq!(team.version, "1.0.0");
        assert_eq!(team.agents.len(), 7);

        // Check that all expected agents exist
        assert!(team.agents.contains_key("time_analyst"));
        assert!(team.agents.contains_key("design_assistant"));
        assert!(team.agents.contains_key("site_coordinator"));
        assert!(team.agents.contains_key("scheduler"));
        assert!(team.agents.contains_key("client_shield"));
        assert!(team.agents.contains_key("northern_envelope"));
        assert!(team.agents.contains_key("the_closer"));

        // Check routing rules exist
        assert_eq!(team.routing.len(), 7);

        // Check agent descriptions
        let time_analyst = &team.agents["time_analyst"];
        assert!(time_analyst.description.contains("Time & Value Analyst"));
        assert!(time_analyst.description.contains("estimates"));

        let design_assistant = &team.agents["design_assistant"];
        assert!(design_assistant.description.contains("Design Assistant"));
        assert!(design_assistant.description.contains("RevitMCP"));

        let northern_envelope = &team.agents["northern_envelope"];
        assert!(northern_envelope.description.contains("Northern Envelope"));
        assert!(northern_envelope.description.contains("cold climates"));
    }

    #[test]
    fn test_the_closer_routing_simple() {
        let registry = TeamRegistry::new("../teams");
        let team = registry.load_team_config(Path::new("../teams/design-build-firm.toml")).unwrap();
        let router = Router::new(team);

        // Test with a simple message
        let result = router.classify("generate the punch list");
        assert_eq!(result.intent, "the_closer");

        let result = router.classify("complete the work");
        assert_eq!(result.intent, "the_closer");
    }

    #[test]
    fn test_design_build_firm_routing() {
        let registry = TeamRegistry::new("../teams");
        let team = registry.load_team_config(Path::new("../teams/design-build-firm.toml")).unwrap();
        let router = Router::new(team);

        // Test time analyst routing
        let result = router.classify("How long will this bathroom reno take and what's the cost estimate?");
        assert_eq!(result.intent, "time_analyst");

        // Test design assistant routing
        let result = router.classify("Can you help me design and create a 3D Revit model for the new addition?");
        assert_eq!(result.intent, "design_assistant");

        // Test northern envelope routing
        let result = router.classify("Will this wall assembly meet SB-12 thermal compliance requirements?");
        assert_eq!(result.intent, "northern_envelope");

        // Test site coordinator routing
        let result = router.classify("What's happening on the job site today with the electrician and plumber?");
        assert_eq!(result.intent, "site_coordinator");

        // Test scheduler routing
        let result = router.classify("What's the critical path schedule timeline for finishing on time?");
        assert_eq!(result.intent, "scheduler");

        // Test client shield routing
        let result = router.classify("Send the customer a communication about scope changes");
        assert_eq!(result.intent, "client_shield");

        // Test the closer routing
        let result = router.classify("Generate the warranty package and final punch list for handoff");
        assert_eq!(result.intent, "the_closer");
    }

    #[test]
    fn test_role_assignment() {
        let registry = TeamRegistry::new("../teams");
        let team = registry.load_team_config(Path::new("../teams/design-build-firm.toml")).unwrap();

        // Create a test runtime for async operations
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Test assigning a role
        rt.block_on(async {
            // First add the team to the registry
            registry.upsert(team.clone()).await;

            let assignment = registry
                .assign_role("design-build-firm", "time_analyst", "custom-agent-123", "test-user")
                .await
                .unwrap();
            let _assignment = registry
                .assign_role("design-build-firm", "time_analyst", "custom-agent-123", "test-user")
                .await
                .unwrap();

            assert_eq!(_assignment.team_id, "design-build-firm");
            assert_eq!(assignment.intent, "time_analyst");
            assert_eq!(assignment.agent_id, "custom-agent-123");

            // Test retrieving the assignment
            let retrieved = registry
                .get_role_assignment("design-build-firm", "time_analyst")
                .await
                .unwrap();

            assert_eq!(retrieved.agent_id, "custom-agent-123");

            // Test resolving agent (should return assigned agent, not default)
            let resolved = registry
                .resolve_agent("design-build-firm", "time_analyst")
                .await
                .unwrap();

            assert_eq!(resolved, "custom-agent-123");

            // Test removing assignment
            let removed = registry
                .remove_role("design-build-firm", "time_analyst")
                .await
                .unwrap();

            assert_eq!(removed.agent_id, "custom-agent-123");

            // After removal, should fall back to default agent
            let resolved = registry
                .resolve_agent("design-build-firm", "time_analyst")
                .await
                .unwrap();

            assert_eq!(resolved, "time-analyst"); // Default from config
        });
    }

    #[test]
    fn test_list_team_assignments() {
        let registry = TeamRegistry::new("../teams");
        let team = registry.load_team_config(Path::new("../teams/design-build-firm.toml")).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            // First add the team to the registry
            registry.upsert(team.clone()).await;

            // Assign multiple roles
            // Assign multiple roles
            registry
                .assign_role("design-build-firm", "time_analyst", "agent-1", "test-user")
                .await
                .unwrap();
            registry
                .assign_role("design-build-firm", "design_assistant", "agent-2", "test-user")
                .await
                .unwrap();
            registry
                .assign_role("design-build-firm", "site_coordinator", "agent-3", "test-user")
                .await
                .unwrap();

            // List all assignments
            let assignments = registry.list_team_assignments("design-build-firm").await;

            assert_eq!(assignments.len(), 3);

            // Verify each assignment
            let assignment_map: std::collections::HashMap<_, _> = assignments
                .iter()
                .map(|a| (a.intent.clone(), a.agent_id.clone()))
                .collect();

            assert_eq!(assignment_map.get("time_analyst").unwrap(), "agent-1");
            assert_eq!(assignment_map.get("design_assistant").unwrap(), "agent-2");
            assert_eq!(assignment_map.get("site_coordinator").unwrap(), "agent-3");
        });
    }

    #[test]
    fn test_assign_invalid_role() {
        let registry = TeamRegistry::new("../teams");

        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            // Try to assign to non-existent intent
            let result = registry
                .assign_role("design-build-firm", "nonexistent_intent", "agent-1", "test-user")
                .await;

            assert!(result.is_err());
        });
    }
}
