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

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateConfig {
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    #[serde(default = "default_cpu")]
    pub cpu_cores: f32,
}

fn default_memory() -> u32 {
    1024
}
fn default_cpu() -> f32 {
    1.0
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
