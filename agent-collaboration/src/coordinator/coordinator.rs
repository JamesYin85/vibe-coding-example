//! Coordinator Agent - Orchestrates task decomposition and multi-agent collaboration
//!
//! The Coordinator receives tasks, decomposes them using TaskDecomposer,
//! assigns subtasks to appropriate specialized agents, and coordinates execution.

use crate::agent::{Agent, BaseAgent, Output, SubTask, Task, AgentState};
use crate::capability::CapabilityRegistry;
use crate::communication::Message;
use crate::coordinator::result::{ParallelExecutionResult, SubtaskResult};
use crate::decomposer::{
    AgentAssigner, AgentInfo, AssignmentStrategy,
    HybridStrategy, TaskAnalysis, TaskAnalyzer, DecomposeStrategy,
};
use crate::error::{AgentError, Result};
use crate::llm::{FallbackClient, LLMConfig};
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, instrument};

use super::specialized::{
    CodeAnalysisAgent, CodePerformanceAgent, CodeSecurityAgent,
    CodeStyleAgent, CodeStructureAgent, SpecializedAgent,
};

/// Execution result from the coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub task_id: String,
    pub task_description: String,
    pub analysis: TaskAnalysis,
    pub subtask_count: usize,
    pub agent_outputs: HashMap<String, Output>,
    pub summary: Value,
    pub success: bool,
    /// Failed subtasks with error details
    pub failed_subtasks: Vec<SubtaskResult>,
    /// Whether this was a partial success (some succeeded, some failed)
    pub partial_success: bool,
}

/// Coordinator configuration
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub use_llm_for_decomposition: bool,
    pub parallel_execution: bool,
    pub max_agents: usize,
    /// Maximum number of subtasks to execute concurrently
    pub max_concurrent_subtasks: usize,
    /// If true, return immediately on first error (old behavior)
    /// If false, continue executing and collect partial results
    pub fail_fast: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            use_llm_for_decomposition: false,
            parallel_execution: true,
            max_agents: 10,
            max_concurrent_subtasks: 4,
            fail_fast: false,
        }
    }
}

/// Coordinator Agent - Orchestrates multi-agent task execution
pub struct Coordinator {
    base: BaseAgent,
    config: CoordinatorConfig,
    strategy: HybridStrategy,
    assigner: AgentAssigner,
    specialized_agents: HashMap<String, Arc<RwLock<Box<dyn SpecializedAgent>>>>,
    agent_registry: Vec<AgentInfo>,
    llm_client: Option<Arc<FallbackClient>>,
}

impl Coordinator {
    pub fn new(id: &str) -> Self {
        Self::with_config(id, CoordinatorConfig::default())
    }

    pub fn with_config(id: &str, config: CoordinatorConfig) -> Self {
        let mut coordinator = Self {
            base: BaseAgent::new(id, "Coordinator"),
            config,
            strategy: HybridStrategy::new(),
            assigner: AgentAssigner::new().with_strategy(AssignmentStrategy::CapabilityBased),
            specialized_agents: HashMap::new(),
            agent_registry: Vec::new(),
            llm_client: None,
        };

        // Register default specialized agents
        coordinator.register_default_agents();
        coordinator
    }

    pub fn with_llm(mut self, config: LLMConfig) -> Result<Self> {
        let client = FallbackClient::new(config)
            .map_err(|e| AgentError::internal(format!("Failed to create LLM client: {}", e)))?;
        self.llm_client = Some(Arc::new(client));
        Ok(self)
    }

    fn register_default_agents(&mut self) {
        // Register code analysis agents
        self.register_specialized_agent(Box::new(CodeStyleAgent::new("style-agent")));
        self.register_specialized_agent(Box::new(CodeSecurityAgent::new("security-agent")));
        self.register_specialized_agent(Box::new(CodePerformanceAgent::new("perf-agent")));
        self.register_specialized_agent(Box::new(CodeStructureAgent::new("structure-agent")));
        self.register_specialized_agent(Box::new(CodeAnalysisAgent::new("general-analyzer")));
    }

    fn register_specialized_agent(&mut self, agent: Box<dyn SpecializedAgent>) {
        let info = AgentInfo::new(agent.id(), agent.name())
            .with_capabilities(agent.expertise().into_iter().map(|s| s.to_string()).collect())
            .with_max_load(5);

        let id = agent.id().to_string();
        self.agent_registry.push(info);
        self.specialized_agents.insert(id.clone(), Arc::new(RwLock::new(agent)));

        debug!("Registered specialized agent: {}", id);
    }

    /// Analyze and decompose a task
    #[instrument(skip(self, task))]
    async fn analyze_and_decompose(&self, task: &Task) -> Result<(TaskAnalysis, Vec<SubTask>)> {
        info!(task_id = %task.id, "Analyzing and decomposing task");

        // Analyze the task using TaskAnalyzer
        let analysis = TaskAnalyzer::analyze(task)?;

        info!(
            task_id = %task.id,
            complexity = ?analysis.complexity,
            estimated_steps = analysis.estimated_steps,
            "Task analysis completed"
        );

        // Decompose using hybrid strategy
        let result = self.strategy.decompose(task, &self.agent_registry).await?;

        info!(
            task_id = %task.id,
            subtask_count = result.subtasks.len(),
            "Task decomposed and agents assigned"
        );

        Ok((analysis, result.subtasks))
    }

    /// Execute a single subtask with the appropriate agent
    #[instrument(skip(self, subtask))]
    async fn execute_subtask(&self, subtask: &SubTask) -> Result<Output> {
        let agent_id = subtask.assigned_to.as_ref()
            .ok_or_else(|| AgentError::internal("Subtask not assigned to any agent"))?;

        let agent = self.specialized_agents.get(agent_id)
            .ok_or_else(|| AgentError::internal(format!("Agent {} not found", agent_id)))?;

        let mut agent = agent.write().await;

        info!(
            subtask_id = %subtask.id,
            agent_id = %agent_id,
            agent_name = %agent.name(),
            "Executing subtask with agent"
        );

        // Set agent state to running
        agent.set_state(AgentState::Running);

        // Execute the subtask
        let result = agent.execute(subtask.clone()).await;

        match &result {
            Ok(output) => {
                info!(
                    subtask_id = %subtask.id,
                    success = output.success,
                    "Subtask completed successfully"
                );
                agent.set_state(AgentState::Completed);
            }
            Err(e) => {
                warn!(
                    subtask_id = %subtask.id,
                    error = %e,
                    "Subtask execution failed"
                );
                agent.set_state(AgentState::Failed);
            }
        }

        result
    }

    /// Execute all subtasks (parallel or sequential based on config)
    /// Returns ParallelExecutionResult with both successes and failures
    #[instrument(skip(self, subtasks))]
    async fn execute_subtasks_with_partial_failure(
        &self,
        subtasks: &[SubTask],
    ) -> ParallelExecutionResult {
        let total = subtasks.len();

        if self.config.parallel_execution {
            // Execute with bounded concurrency using buffer_unordered
            let results: Vec<SubtaskResult> = stream::iter(subtasks.iter())
                .map(|subtask| async {
                    let agent_id = subtask.assigned_to.clone().unwrap_or_default();
                    match self.execute_subtask(subtask).await {
                        Ok(output) => SubtaskResult::success(
                            subtask.id.clone(),
                            agent_id,
                            output,
                        ),
                        Err(e) => SubtaskResult::failure(
                            subtask.id.clone(),
                            agent_id,
                            e.to_string(),
                        ),
                    }
                })
                .buffer_unordered(self.config.max_concurrent_subtasks)
                .collect()
                .await;

            ParallelExecutionResult::from_results(results)
        } else {
            // Execute sequentially, collecting results
            let mut execution_result = ParallelExecutionResult::with_capacity(total);

            for subtask in subtasks {
                let agent_id = subtask.assigned_to.clone().unwrap_or_default();
                match self.execute_subtask(subtask).await {
                    Ok(output) => {
                        execution_result.add_success(subtask.id.clone(), output);
                    }
                    Err(e) => {
                        execution_result.add_failure(SubtaskResult::failure(
                            subtask.id.clone(),
                            agent_id,
                            e.to_string(),
                        ));
                    }
                }
            }

            execution_result
        }
    }

    /// Execute all subtasks (parallel or sequential based on config)
    /// Legacy method that fails fast on first error for backward compatibility
    #[instrument(skip(self, subtasks))]
    async fn execute_subtasks(
        &self,
        subtasks: &[SubTask],
    ) -> Result<HashMap<String, Output>> {
        // Use fail_fast mode if configured
        if self.config.fail_fast {
            self.execute_subtasks_fail_fast(subtasks).await
        } else {
            // Use partial failure handling, but return error if all failed
            let result = self.execute_subtasks_with_partial_failure(subtasks).await;

            if result.is_complete_failure() {
                // Return the first error
                if let Some(failed) = result.failed.first() {
                    if let Some(error_msg) = &failed.error_message {
                        return Err(AgentError::internal(error_msg));
                    }
                }
                return Err(AgentError::internal("All subtasks failed"));
            }

            Ok(result.successful)
        }
    }

    /// Execute subtasks with fail-fast behavior (legacy)
    #[instrument(skip(self, subtasks))]
    async fn execute_subtasks_fail_fast(
        &self,
        subtasks: &[SubTask],
    ) -> Result<HashMap<String, Output>> {
        let mut outputs = HashMap::new();

        if self.config.parallel_execution {
            // Execute all subtasks in parallel
            let mut futures = Vec::new();

            for subtask in subtasks {
                futures.push(self.execute_subtask(subtask));
            }

            // Execute all subtasks concurrently
            let results = futures::future::join_all(futures).await;

            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Ok(output) => {
                        outputs.insert(format!("subtask_{}", i), output);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        } else {
            // Execute sequentially
            for subtask in subtasks {
                let output = self.execute_subtask(subtask).await?;
                outputs.insert(subtask.id.clone(), output);
            }
        }

        Ok(outputs)
    }

    /// Generate a summary of the execution results
    fn generate_summary(&self, result: &ExecutionResult) -> Value {
        let mut domain_scores = HashMap::new();
        let mut all_issues = Vec::new();

        for (agent_id, output) in &result.agent_outputs {
            if let Some(domain) = output.result.get("domain").and_then(|d| d.as_str()) {
                if let Some(score) = output.result.get("score").and_then(|s| s.as_u64()) {
                    domain_scores.insert(domain.to_string(), score as u32);
                }
            }

            if let Some(issues) = output.result.get("issues").and_then(|i| i.as_array()) {
                for issue in issues {
                    all_issues.push(json!({
                        "agent": agent_id,
                        "issue": issue
                    }));
                }
            }

            if let Some(vulnerabilities) = output.result.get("vulnerabilities").and_then(|v| v.as_array()) {
                for vuln in vulnerabilities {
                    all_issues.push(json!({
                        "agent": agent_id,
                        "vulnerability": vuln
                    }));
                }
            }
        }

        let overall_score = if !domain_scores.is_empty() {
            domain_scores.values().sum::<u32>() / domain_scores.len() as u32
        } else {
            0
        };

        json!({
            "overall_score": overall_score,
            "domain_scores": domain_scores,
            "total_issues": all_issues.len(),
            "issues": all_issues,
            "agent_count": result.agent_outputs.len(),
            "recommendations": self.generate_recommendations(overall_score, &all_issues)
        })
    }

    fn generate_recommendations(&self, score: u32, issues: &[Value]) -> Vec<String> {
        let mut recommendations = Vec::new();

        if score < 50 {
            recommendations.push("代码质量需要重点关注，建议进行全面重构".to_string());
        } else if score < 70 {
            recommendations.push("代码质量一般，建议针对性优化".to_string());
        } else if score < 90 {
            recommendations.push("代码质量良好，可以进一步优化细节".to_string());
        } else {
            recommendations.push("代码质量优秀，继续保持".to_string());
        }

        // Add specific recommendations based on issues
        for issue in issues {
            if let Some(severity) = issue.get("vulnerability").and_then(|v| v.get("severity")).and_then(|s| s.as_str()) {
                if severity == "critical" {
                    recommendations.push("发现严重安全问题，需要立即修复".to_string());
                }
            }
        }

        recommendations
    }

    /// Main entry point - process a task
    #[instrument(skip(self))]
    pub async fn process(&mut self, input: &str) -> Result<ExecutionResult> {
        info!(input = %input, "Coordinator processing task");

        // Step 1: Understand the task
        let task = self.understand(input).await?;
        info!(task_id = %task.id, "Task understood");

        // Step 2: Analyze and decompose
        let (analysis, subtasks) = self.analyze_and_decompose(&task).await?;

        // Step 3: Execute subtasks
        let agent_outputs = self.execute_subtasks(&subtasks).await?;

        // Step 4: Generate summary
        let mut result = ExecutionResult {
            task_id: task.id.clone(),
            task_description: task.description.clone(),
            analysis,
            subtask_count: subtasks.len(),
            agent_outputs,
            summary: json!(null),
            success: true,
            failed_subtasks: Vec::new(),
            partial_success: false,
        };

        result.summary = self.generate_summary(&result);

        info!(
            task_id = %result.task_id,
            agent_count = result.agent_outputs.len(),
            "Task processing completed"
        );

        Ok(result)
    }

    /// Process code analysis task with explicit code
    #[instrument(skip(self, code))]
    pub async fn analyze_code(&mut self, code: &str) -> Result<ExecutionResult> {
        info!(code_len = code.len(), "Analyzing code quality");

        // Create task with code context
        let task = Task {
            id: format!("code-analysis-{}", uuid::Uuid::new_v4()),
            description: "Analyze code quality".to_string(),
            context: json!({
                "code": code,
                "analysis_type": "quality",
                "steps": [
                    {"name": "style_analysis", "depends_on": []},
                    {"name": "security_analysis", "depends_on": []},
                    {"name": "performance_analysis", "depends_on": []},
                    {"name": "structure_analysis", "depends_on": []},
                    {"name": "final_summary", "depends_on": [0, 1, 2, 3]}
                ]
            }),
        };

        // Analyze and decompose
        let (analysis, mut subtasks) = self.analyze_and_decompose(&task).await?;

        // Add code to all subtask parameters
        for subtask in &mut subtasks {
            subtask.parameters = json!({
                "code": code,
                "original_description": subtask.description
            });
        }

        // Execute subtasks
        let agent_outputs = self.execute_subtasks(&subtasks).await?;

        // Generate summary
        let mut result = ExecutionResult {
            task_id: task.id,
            task_description: task.description,
            analysis,
            subtask_count: subtasks.len(),
            agent_outputs,
            summary: json!(null),
            success: true,
            failed_subtasks: Vec::new(),
            partial_success: false,
        };

        result.summary = self.generate_summary(&result);

        Ok(result)
    }
}

#[async_trait]
impl Agent for Coordinator {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        let (_, subtasks) = self.analyze_and_decompose(&task).await?;
        Ok(subtasks)
    }

    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        self.base.execute(subtask).await
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = Coordinator::new("test-coordinator");
        assert_eq!(coordinator.id(), "test-coordinator");
        assert!(!coordinator.specialized_agents.is_empty());
    }

    #[tokio::test]
    async fn test_code_analysis() {
        let mut coordinator = Coordinator::new("test-coordinator");

        let code = r#"
fn calculate_sum(numbers: &[i32]) -> i32 {
    let mut sum = 0;
    for n in numbers {
        sum += n;
    }
    sum
}
"#;

        let result = coordinator.analyze_code(code).await.unwrap();

        assert!(result.success);
        assert!(!result.agent_outputs.is_empty());
        assert!(result.summary.get("overall_score").is_some());
    }

    #[tokio::test]
    async fn test_task_decomposition() {
        let mut coordinator = Coordinator::new("test-coordinator");

        let task = Task {
            id: "test-task".to_string(),
            description: "Analyze this code for quality issues".to_string(),
            context: json!({}),
        };

        let subtasks = coordinator.decompose(task).await.unwrap();
        assert!(!subtasks.is_empty());

        // Check that subtasks are assigned
        for subtask in &subtasks {
            assert!(subtask.assigned_to.is_some());
        }
    }
}
