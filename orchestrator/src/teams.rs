//! Team management and routing logic
//!
//! Teams provide a single entry point where a router agent classifies
//! incoming messages and routes them to the appropriate specialist agent.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::*;

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
}

impl TeamRegistry {
    pub fn new(teams_dir: &str) -> Self {
        Self {
            teams: RwLock::new(HashMap::new()),
            teams_dir: teams_dir.to_string(),
        }
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
                let needs_clarification =
                    confidence < self.team.router.confidence_threshold
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
        let mut options: Vec<String> = self
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
}
