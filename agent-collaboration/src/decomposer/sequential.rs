use crate::agent::{SubTask, Task};
use crate::decomposer::{
    AgentAssigner, AgentInfo, AssignmentStrategy, DecomposeStrategy, StrategyType, TaskAnalysis,
    TaskAnalyzer,
};
use crate::error::Result;
use async_trait::async_trait;
use tracing::{debug, info, instrument};
use uuid::Uuid;

pub struct SequentialStrategy {
    assigner: AgentAssigner,
}

impl SequentialStrategy {
    pub fn new() -> Self {
        Self {
            assigner: AgentAssigner::new(),
        }
    }

    pub fn with_assignment_strategy(mut self, strategy: AssignmentStrategy) -> Self {
        self.assigner = AgentAssigner::new().with_strategy(strategy);
        self
    }
}

impl Default for SequentialStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecomposeStrategy for SequentialStrategy {
    fn strategy_type(&self) -> StrategyType {
        StrategyType::Sequential
    }

    #[instrument(skip(self, task))]
    async fn analyze(&self, task: &Task) -> Result<TaskAnalysis> {
        debug!(task_id = %task.id, "Analyzing task for sequential execution");

        let mut analysis = TaskAnalyzer::analyze(task)?;

        // 顺序策略：所有任务按依赖顺序执行
        analysis.suggested_strategy = StrategyType::Sequential;
        analysis.can_parallelize = false;

        info!(
            task_id = %task.id,
            complexity = ?analysis.complexity,
            steps = analysis.estimated_steps,
            "Task analysis complete for sequential strategy"
        );

        Ok(analysis)
    }

    #[instrument(skip(self, analysis))]
    async fn generate(&self, analysis: &TaskAnalysis) -> Result<Vec<SubTask>> {
        debug!(
            task_id = %analysis.task_id,
            steps = analysis.estimated_steps,
            "Generating sequential subtasks"
        );

        let mut subtasks = Vec::with_capacity(analysis.estimated_steps);

        for i in 0..analysis.estimated_steps {
            let subtask = SubTask {
                id: format!("{}_seq_{}", analysis.task_id, Uuid::new_v4()),
                parent_id: analysis.task_id.clone(),
                description: format!("Step {} of {}", i + 1, analysis.estimated_steps),
                parameters: serde_json::json!({
                    "step_index": i,
                    "total_steps": analysis.estimated_steps,
                    "execution_order": i,
                }),
                assigned_to: None,
            };

            debug!(subtask_id = %subtask.id, order = i, "Created sequential subtask");
            subtasks.push(subtask);
        }

        info!(
            task_id = %analysis.task_id,
            subtask_count = subtasks.len(),
            "Generated sequential subtasks"
        );

        Ok(subtasks)
    }

    #[instrument(skip(self, subtasks, agents))]
    async fn assign(
        &self,
        subtasks: &mut [SubTask],
        agents: &[AgentInfo],
    ) -> Result<()> {
        debug!(
            subtask_count = subtasks.len(),
            agent_count = agents.len(),
            "Assigning sequential subtasks to agents"
        );

        // 顺序执行时使用轮询分配
        self.assigner.assign(subtasks, agents)?;

        info!(
            assignments = ?subtasks.iter().map(|s| (&s.id, &s.assigned_to)).collect::<Vec<_>>(),
            "Sequential assignment complete"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_task() -> Task {
        Task {
            id: "test-task".to_string(),
            description: "Test sequential task".to_string(),
            context: json!({
                "steps": [
                    {"name": "step1"},
                    {"name": "step2"},
                    {"name": "step3"}
                ]
            }),
        }
    }

    fn create_test_agents() -> Vec<AgentInfo> {
        vec![
            AgentInfo::new("agent-1", "Worker 1"),
            AgentInfo::new("agent-2", "Worker 2"),
        ]
    }

    #[tokio::test]
    async fn test_sequential_analysis() {
        let strategy = SequentialStrategy::new();
        let task = create_test_task();

        let analysis = strategy.analyze(&task).await.unwrap();
        assert_eq!(analysis.suggested_strategy, StrategyType::Sequential);
        assert!(!analysis.can_parallelize);
    }

    #[tokio::test]
    async fn test_sequential_generate() {
        let strategy = SequentialStrategy::new();
        let task = create_test_task();

        let analysis = strategy.analyze(&task).await.unwrap();
        let subtasks = strategy.generate(&analysis).await.unwrap();

        assert_eq!(subtasks.len(), 3);
        // 验证顺序参数
        for (i, subtask) in subtasks.iter().enumerate() {
            let order = subtask.parameters["execution_order"].as_u64().unwrap();
            assert_eq!(order as usize, i);
        }
    }

    #[tokio::test]
    async fn test_sequential_assign() {
        let strategy = SequentialStrategy::new();
        let task = create_test_task();
        let agents = create_test_agents();

        let result = strategy.decompose(&task, &agents).await.unwrap();

        assert_eq!(result.subtasks.len(), 3);
        assert_eq!(result.strategy_used, StrategyType::Sequential);

        // 所有子任务都应该被分配
        for subtask in &result.subtasks {
            assert!(subtask.assigned_to.is_some());
        }
    }
}
