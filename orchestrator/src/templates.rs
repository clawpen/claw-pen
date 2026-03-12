use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: Option<String>,
    pub config: TemplateConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct TemplateConfig {
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub memory_mb: u32,
    #[serde(default)]
    pub cpu_cores: f32,
    /// Custom container image (e.g., "openclaw-agent:latest")
    /// If not specified, defaults to node:20-alpine
    #[serde(default)]
    pub image: Option<String>,
    /// Volume mount paths inside the container
    /// Host paths will be created automatically under {data_dir}/agents/{agent_name}/
    #[serde(default)]
    pub volumes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TemplatesFile {
    templates: HashMap<String, Template>,
}

pub struct TemplateRegistry {
    templates: HashMap<String, Template>,
}

impl TemplateRegistry {
    pub fn load() -> Result<Self> {
        // Try multiple paths
        let paths = [
            "./templates/agents.yaml",
            "/etc/claw-pen/templates/agents.yaml",
        ];

        for path in &paths {
            if let Ok(contents) = std::fs::read_to_string(path) {
                let file: TemplatesFile = serde_yaml::from_str(&contents)?;
                return Ok(Self {
                    templates: file.templates,
                });
            }
        }

        // No templates file found - use empty registry
        Ok(Self {
            templates: HashMap::new(),
        })
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn list(&self) -> Vec<(&String, &Template)> {
        self.templates.iter().collect()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::load().unwrap_or(Self {
            templates: HashMap::new(),
        })
    }
}
