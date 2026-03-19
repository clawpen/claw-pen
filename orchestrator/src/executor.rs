// Workflow executor for running multi-agent pipelines

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use futures_util::future;

use crate::types::AgentContainer;
use crate::workflow::{
    Workflow, WorkflowExecutionRequest, ExecutionStatus, StepResult,
    StepTask, ExecutionStrategy, WorkflowRegistry,
};

/// Executes workflows by coordinating agent tasks
pub struct WorkflowExecutor {
    /// Workflow registry
    registry: Arc<RwLock<WorkflowRegistry>>,
    /// Available agents
    agents: Arc<RwLock<Vec<AgentContainer>>>,
}

impl WorkflowExecutor {
    pub fn new(
        registry: Arc<RwLock<WorkflowRegistry>>,
        agents: Arc<RwLock<Vec<AgentContainer>>>,
    ) -> Self {
        Self { registry, agents }
    }

    /// Execute a workflow
    pub async fn execute_workflow(&self, request: WorkflowExecutionRequest) -> Result<String> {
        let mut registry = self.registry.write().await;
        let execution_id = registry.start_execution(request)?;

        // Clone the workflow for execution
        let workflow = registry.get_workflow(
            &registry.get_execution(&execution_id).unwrap().workflow_id
        ).unwrap().clone();

        // Drop the write lock before executing
        drop(registry);

        // Clone for the spawn block
        let registry = Arc::clone(&self.registry);
        let agents = Arc::clone(&self.agents);
        let registry_for_handler = Arc::clone(&self.registry);
        let execution_id_for_handler = execution_id.clone();
        let execution_id_for_return = execution_id.clone();

        // Spawn execution in background
        tokio::spawn(async move {
            if let Err(e) = run_workflow(&workflow, execution_id_for_handler.clone(), registry, agents).await {
                tracing::error!("Workflow execution failed: {}", e);

                // Update execution status to failed
                let mut registry = registry_for_handler.write().await;
                if let Some(execution) = registry.get_execution_mut(&execution_id_for_handler) {
                    execution.status = ExecutionStatus::Failed;
                    execution.error = Some(e.to_string());
                    execution.completed_at = Some(Utc::now());
                }
            }
        });

        Ok(execution_id_for_return)
    }
}

/// Run a workflow execution (standalone function for tokio::spawn)
async fn run_workflow(
    workflow: &Workflow,
    execution_id: String,
    registry: Arc<RwLock<WorkflowRegistry>>,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<()> {
    // Update execution status to running
    {
        let mut registry = registry.write().await;
        if let Some(execution) = registry.get_execution_mut(&execution_id) {
            execution.status = ExecutionStatus::Running;
        }
    }

    // Execute based on strategy
    match &workflow.strategy {
        ExecutionStrategy::Sequential => {
            execute_sequential(workflow, &execution_id, registry.clone(), agents.clone()).await?;
        }
        ExecutionStrategy::Parallel => {
            execute_parallel(workflow, &execution_id, registry.clone(), agents.clone()).await?;
        }
        ExecutionStrategy::Dag => {
            execute_dag(workflow, &execution_id, registry.clone(), agents.clone()).await?;
        }
    }

    // Mark execution as completed
    {
        let mut registry = registry.write().await;
        if let Some(execution) = registry.get_execution_mut(&execution_id) {
            execution.status = ExecutionStatus::Completed;
            execution.completed_at = Some(Utc::now());
        }
    }

    Ok(())
}

/// Execute workflow steps sequentially
async fn execute_sequential(
    workflow: &Workflow,
    execution_id: &str,
    registry: Arc<RwLock<WorkflowRegistry>>,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<()> {
    let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();

    for step in &workflow.steps {
        tracing::info!("Executing step: {}", step.name);

        let result = execute_step(step, &step_outputs, execution_id, agents.clone()).await?;

        // Store output for next steps
        if let Some(output) = result.output.clone() {
            step_outputs.insert(step.id.clone(), output);
        }

        // Update execution with step result
        {
            let mut registry = registry.write().await;
            if let Some(execution) = registry.get_execution_mut(execution_id) {
                execution.step_results.insert(step.id.clone(), result);
                execution.current_step = Some(step.id.clone());
            }
        }

        // Check if step failed
        let registry = registry.read().await;
        let execution = registry.get_execution(execution_id).unwrap();
        let step_result = execution.step_results.get(&step.id).unwrap();
        if matches!(step_result.status, ExecutionStatus::Failed) {
            return Err(anyhow::anyhow!("Step {} failed", step.name));
        }
    }

    Ok(())
}

/// Execute workflow steps in parallel (where possible)
async fn execute_parallel(
    workflow: &Workflow,
    execution_id: &str,
    registry: Arc<RwLock<WorkflowRegistry>>,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<()> {
    // Group steps by dependency level
    let levels = get_dependency_levels(workflow);

    let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();

    // Execute each level sequentially, but steps within level in parallel
    for level_steps in levels {
        let mut tasks = Vec::new();

        for step_id in level_steps {
            let step = workflow.steps.iter().find(|s| s.id == step_id).unwrap();
            let exec_id = execution_id.to_string();
            let outputs = step_outputs.clone();
            let step_clone = step.clone();
            let agents_clone = agents.clone();
            let registry_clone = registry.clone();

            tasks.push(tokio::spawn(async move {
                let result = execute_step(&step_clone, &outputs, &exec_id, agents_clone).await?;

                // Update execution with step result
                {
                    let mut registry = registry_clone.write().await;
                    if let Some(execution) = registry.get_execution_mut(&exec_id) {
                        execution.step_results.insert(step_clone.id.clone(), result.clone());
                        execution.current_step = Some(step_clone.id.clone());
                    }
                }

                Ok::<(String, StepResult), anyhow::Error>((step_clone.id.clone(), result))
            }));
        }

        // Wait for all tasks in this level to complete
        let results = future::join_all(tasks).await;

        for result in results {
            let (step_id, step_result) = result??;
            if let Some(output) = step_result.output {
                step_outputs.insert(step_id.clone(), output);
            }

            if matches!(step_result.status, ExecutionStatus::Failed) {
                return Err(anyhow::anyhow!("Step {} failed", step_id));
            }
        }
    }

    Ok(())
}

/// Execute workflow steps as a DAG (respecting dependencies)
async fn execute_dag(
    workflow: &Workflow,
    execution_id: &str,
    registry: Arc<RwLock<WorkflowRegistry>>,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<()> {
    // DAG execution is similar to parallel, but we use the depends_on field directly
    execute_parallel(workflow, execution_id, registry, agents).await
}

/// Execute a single workflow step
async fn execute_step(
    step: &crate::workflow::WorkflowStep,
    step_outputs: &HashMap<String, serde_json::Value>,
    _execution_id: &str,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<StepResult> {
    let start_time = std::time::Instant::now();
    let mut attempt = 0;
    let max_attempts = step.retry_policy.max_attempts;

    // Resolve agent for this step
    let agent = resolve_agent(step, agents.clone()).await?;

    // Prepare task with inputs
    let _task = prepare_task(step, step_outputs)?;

    loop {
        attempt += 1;

        // Execute the task
        let execution_result = execute_task_on_agent(&agent, step).await;

        match execution_result {
            Ok(output) => {
                let duration = start_time.elapsed().as_millis() as u64;
                return Ok(StepResult {
                    step_id: step.id.clone(),
                    status: ExecutionStatus::Completed,
                    output: Some(output),
                    timestamp: Utc::now(),
                    duration_ms: duration,
                    error: None,
                });
            }
            Err(e) => {
                if attempt >= max_attempts {
                    let duration = start_time.elapsed().as_millis() as u64;
                    return Ok(StepResult {
                        step_id: step.id.clone(),
                        status: ExecutionStatus::Failed,
                        output: None,
                        timestamp: Utc::now(),
                        duration_ms: duration,
                        error: Some(format!("Failed after {} attempts: {}", attempt, e)),
                    });
                }

                // Exponential backoff
                let delay = (step.retry_policy.initial_delay_ms as f32
                    * step.retry_policy.backoff_multiplier.powi(attempt as i32 - 1))
                    as u64;
                let delay = delay.min(step.retry_policy.max_delay_ms);

                tracing::warn!(
                    "Step {} failed (attempt {}/{}), retrying in {}ms: {}",
                    step.name,
                    attempt,
                    max_attempts,
                    delay,
                    e
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
        }
    }
}

/// Resolve which agent should execute a step
async fn resolve_agent(
    step: &crate::workflow::WorkflowStep,
    agents: Arc<RwLock<Vec<AgentContainer>>>,
) -> Result<AgentContainer> {
    let agents = agents.read().await;

    // If agent_id is specified, use that agent
    if let Some(agent_id) = &step.agent_id {
        return agents
            .iter()
            .find(|a| &a.id == agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_id)).cloned();
    }

    // Otherwise, find an agent matching the pattern
    if let Some(pattern) = &step.agent_pattern {
        // Simple pattern matching (can be enhanced)
        return agents
            .iter()
            .filter(|a| a.status == crate::types::AgentStatus::Running)
            .find(|a| {
                a.name.contains(pattern)
                    || a.tags.iter().any(|t| t.contains(pattern))
            })
            .ok_or_else(|| anyhow::anyhow!("No agent matching pattern {}", pattern)).cloned();
    }

    // Default: use first available running agent
    agents
        .iter()
        .find(|a| a.status == crate::types::AgentStatus::Running)
        .ok_or_else(|| anyhow::anyhow!("No running agents available")).cloned()
}

/// Prepare task with inputs from previous steps
fn prepare_task(
    _step: &crate::workflow::WorkflowStep,
    _step_outputs: &HashMap<String, serde_json::Value>,
) -> Result<StepTask> {
    // For now, just return a placeholder
    // TODO: Implement template substitution for step inputs
    Err(anyhow::anyhow!("Task preparation not yet implemented"))
}

/// Execute a task on an agent
async fn execute_task_on_agent(
    _agent: &AgentContainer,
    _step: &crate::workflow::WorkflowStep,
) -> Result<serde_json::Value> {
    // TODO: Implement actual agent communication
    // For now, return a success response
    Ok(serde_json::json!({
        "status": "success",
        "message": "Task executed (placeholder)"
    }))
}

/// Get dependency levels for parallel execution
fn get_dependency_levels(workflow: &Workflow) -> Vec<Vec<String>> {
    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();

    while placed.len() < workflow.steps.len() {
        let mut current_level = Vec::new();

        for step in &workflow.steps {
            if placed.contains(&step.id) {
                continue;
            }

            // Check if all dependencies are placed
            let deps_satisfied = step
                .depends_on
                .iter()
                .all(|dep| placed.contains(dep));

            if deps_satisfied {
                current_level.push(step.id.clone());
            }
        }

        if current_level.is_empty() {
            // Circular dependency or error
            break;
        }

        for step_id in &current_level {
            placed.insert(step_id.clone());
        }

        levels.push(current_level);
    }

    levels
}
