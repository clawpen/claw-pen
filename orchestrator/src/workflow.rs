// Workflow orchestration system for multi-agent pipelines
// Enables complex task chains and agent collaboration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// A workflow definition for orchestrating multi-agent tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow identifier
    pub id: String,
    /// Human-readable workflow name
    pub name: String,
    /// Workflow description
    pub description: String,
    /// List of workflow steps
    pub steps: Vec<WorkflowStep>,
    /// Workflow execution strategy
    pub strategy: ExecutionStrategy,
    /// Workflow metadata
    pub metadata: WorkflowMetadata,
}

/// How to execute the workflow steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// Execute steps sequentially (each step waits for previous to complete)
    Sequential,
    /// Execute steps in parallel where possible
    Parallel,
    /// Custom dependency graph (steps specify their dependencies)
    Dag,
}

/// A single step in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step identifier (unique within workflow)
    pub id: String,
    /// Step name
    pub name: String,
    /// Agent ID to execute this step (null = orchestrator assigns)
    pub agent_id: Option<String>,
    /// Agent type/name pattern for auto-assignment
    pub agent_pattern: Option<String>,
    /// Step to execute (prompt, command, etc.)
    pub task: StepTask,
    /// Dependencies - step IDs that must complete before this step
    pub depends_on: Vec<String>,
    /// Input data mapping (references outputs from previous steps)
    pub inputs: StepInputs,
    /// Output mapping (how to store step results)
    pub outputs: StepOutputs,
    /// Retry configuration
    pub retry_policy: RetryPolicy,
    /// Timeout for this step (seconds)
    pub timeout: u64,
}

/// The task to execute in a workflow step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StepTask {
    /// Chat completion task
    Chat {
        /// The prompt/template to send to the agent
        prompt: String,
        /// Optional system message
        system_message: Option<String>,
        /// Temperature for generation
        temperature: Option<f32>,
        /// Max tokens to generate
        max_tokens: Option<u32>,
    },
    /// Code execution task
    ExecuteCode {
        /// Code to execute
        code: String,
        /// Language (python, javascript, etc.)
        language: String,
    },
    /// HTTP request task
    HttpRequest {
        /// URL to request
        url: String,
        /// HTTP method
        method: String,
        /// Request headers
        headers: Option<HashMap<String, String>>,
        /// Request body
        body: Option<serde_json::Value>,
    },
    /// Custom task type
    Custom {
        /// Task type name
        task_type: String,
        /// Task parameters
        parameters: HashMap<String, serde_json::Value>,
    },
}

/// Input data mapping for a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInputs {
    /// Static input values
    #[serde(rename = "static")]
    pub static_values: HashMap<String, serde_json::Value>,
    /// Dynamic inputs from previous step outputs
    /// Format: {"step_id.output_key": "local_var_name"}
    pub from_steps: HashMap<String, String>,
}

/// Output mapping for a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutputs {
    /// Which outputs to store
    pub store: Vec<String>,
    /// Output format (json, text, binary)
    pub format: OutputFormat,
}

/// Output format for step results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    Json,
    Text,
    Binary,
}

/// Retry policy for a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial retry delay (ms)
    pub initial_delay_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f32,
    /// Maximum retry delay (ms)
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

/// Workflow metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// Workflow version
    pub version: String,
    /// Workflow author
    pub author: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Tags for organizing workflows
    pub tags: Vec<String>,
    /// Whether workflow is active
    pub enabled: bool,
}

impl Default for WorkflowMetadata {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            author: None,
            created_at: Utc::now(),
            tags: Vec::new(),
            enabled: true,
        }
    }
}

/// Workflow execution instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecution {
    /// Unique execution ID
    pub id: String,
    /// Workflow ID being executed
    pub workflow_id: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Current step being executed
    pub current_step: Option<String>,
    /// Step execution results
    pub step_results: HashMap<String, StepResult>,
    /// Execution start time
    pub started_at: DateTime<Utc>,
    /// Execution end time (null if running)
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Workflow execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Result of a workflow step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step ID
    pub step_id: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Step output data
    pub output: Option<serde_json::Value>,
    /// Execution timestamp
    pub timestamp: DateTime<Utc>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Workflow execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionRequest {
    /// Workflow ID to execute
    pub workflow_id: String,
    /// Optional input parameters for the workflow
    pub parameters: HashMap<String, serde_json::Value>,
    /// Optional execution strategy override
    pub strategy: Option<ExecutionStrategy>,
}

/// Workflow registry for managing workflow definitions
pub struct WorkflowRegistry {
    workflows: HashMap<String, Workflow>,
    pub executions: HashMap<String, WorkflowExecution>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            executions: HashMap::new(),
        }
    }

    /// Register a workflow definition
    pub fn register_workflow(&mut self, workflow: Workflow) -> Result<(), anyhow::Error> {
        if self.workflows.contains_key(&workflow.id) {
            return Err(anyhow::anyhow!("Workflow {} already exists", workflow.id));
        }

        // Validate workflow
        self.validate_workflow(&workflow)?;

        self.workflows.insert(workflow.id.clone(), workflow);
        Ok(())
    }

    /// Get a workflow by ID
    pub fn get_workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.get(id)
    }

    /// List all workflows
    pub fn list_workflows(&self) -> Vec<&Workflow> {
        self.workflows.values().collect()
    }

    /// Start executing a workflow
    pub fn start_execution(
        &mut self,
        request: WorkflowExecutionRequest,
    ) -> Result<String, anyhow::Error> {
        let workflow = self
            .workflows
            .get(&request.workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow {} not found", request.workflow_id))?;

        if !workflow.metadata.enabled {
            return Err(anyhow::anyhow!("Workflow is disabled"));
        }

        let execution_id = uuid::Uuid::new_v4().to_string();
        let execution = WorkflowExecution {
            id: execution_id.clone(),
            workflow_id: workflow.id.clone(),
            status: ExecutionStatus::Pending,
            current_step: None,
            step_results: HashMap::new(),
            started_at: Utc::now(),
            completed_at: None,
            error: None,
        };

        self.executions.insert(execution_id.clone(), execution);
        Ok(execution_id)
    }

    /// Get an execution by ID
    pub fn get_execution(&self, id: &str) -> Option<&WorkflowExecution> {
        self.executions.get(id)
    }

    /// Get mutable execution by ID
    pub fn get_execution_mut(&mut self, id: &str) -> Option<&mut WorkflowExecution> {
        self.executions.get_mut(id)
    }

    /// Update execution status
    pub fn update_execution(&mut self, execution: WorkflowExecution) {
        self.executions.insert(execution.id.clone(), execution);
    }

    /// Validate a workflow definition
    fn validate_workflow(&self, workflow: &Workflow) -> Result<(), anyhow::Error> {
        // Check for duplicate step IDs
        let mut step_ids = std::collections::HashSet::new();
        for step in &workflow.steps {
            if !step_ids.insert(&step.id) {
                return Err(anyhow::anyhow!("Duplicate step ID: {}", step.id));
            }

            // Validate dependencies exist
            for dep in &step.depends_on {
                if !workflow.steps.iter().any(|s| &s.id == dep) {
                    return Err(anyhow::anyhow!(
                        "Step {} depends on non-existent step {}",
                        step.id, dep
                    ));
                }
            }
        }

        // Check for circular dependencies
        if self.has_circular_dependencies(workflow) {
            return Err(anyhow::anyhow!("Workflow has circular dependencies"));
        }

        Ok(())
    }

    /// Check if workflow has circular dependencies
    fn has_circular_dependencies(&self, workflow: &Workflow) -> bool {
        // Build dependency graph using owned Strings
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        for step in &workflow.steps {
            graph.insert(step.id.clone(), step.depends_on.to_vec());
        }

        // Detect cycles using DFS
        let mut visited = std::collections::HashSet::new();
        let mut rec_stack = std::collections::HashSet::new();

        for step_id in graph.keys() {
            if self.has_cycle_util(&graph, step_id, &mut visited, &mut rec_stack) {
                return true;
            }
        }

        false
    }

    fn has_cycle_util(
        &self,
        graph: &HashMap<String, Vec<String>>,
        node: &str,
        visited: &mut std::collections::HashSet<String>,
        rec_stack: &mut std::collections::HashSet<String>,
    ) -> bool {
        if rec_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }

        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if self.has_cycle_util(graph, neighbor, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        false
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_validation() {
        let registry = WorkflowRegistry::new();

        let workflow = Workflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            description: "A test workflow".to_string(),
            steps: vec![],
            strategy: ExecutionStrategy::Sequential,
            metadata: WorkflowMetadata::default(),
        };

        assert!(registry.validate_workflow(&workflow).is_ok());
    }

    #[test]
    fn test_circular_dependency_detection() {
        let registry = WorkflowRegistry::new();

        let workflow = Workflow {
            id: "circular-workflow".to_string(),
            name: "Circular Workflow".to_string(),
            description: "A workflow with circular dependencies".to_string(),
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    name: "Step 1".to_string(),
                    agent_id: None,
                    agent_pattern: None,
                    task: StepTask::Chat {
                        prompt: "Test".to_string(),
                        system_message: None,
                        temperature: None,
                        max_tokens: None,
                    },
                    depends_on: vec!["step2".to_string()],
                    inputs: StepInputs {
                        static_values: HashMap::new(),
                        from_steps: HashMap::new(),
                    },
                    outputs: StepOutputs {
                        store: vec![],
                        format: OutputFormat::Json,
                    },
                    retry_policy: RetryPolicy::default(),
                    timeout: 60,
                },
                WorkflowStep {
                    id: "step2".to_string(),
                    name: "Step 2".to_string(),
                    agent_id: None,
                    agent_pattern: None,
                    task: StepTask::Chat {
                        prompt: "Test".to_string(),
                        system_message: None,
                        temperature: None,
                        max_tokens: None,
                    },
                    depends_on: vec!["step1".to_string()],
                    inputs: StepInputs {
                        static_values: HashMap::new(),
                        from_steps: HashMap::new(),
                    },
                    outputs: StepOutputs {
                        store: vec![],
                        format: OutputFormat::Json,
                    },
                    retry_policy: RetryPolicy::default(),
                    timeout: 60,
                },
            ],
            strategy: ExecutionStrategy::Dag,
            metadata: WorkflowMetadata::default(),
        };

        assert!(registry.has_circular_dependencies(&workflow));
    }
}
