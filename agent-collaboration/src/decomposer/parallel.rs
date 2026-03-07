use crate::agent::{SubTask, Task};
use crate::decomposer::{
    AgentAssigner, AgentInfo, AssignmentStrategy, DecomposeStrategy, StrategyType, TaskAnalysis,
    TaskAnalyzer,
};
use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, instrument};
use uuid::Uuid;

pub struct ParallelStrategy {
    assigner: AgentAssigner,
    max_parallel: usize,
}

impl ParallelStrategy {
    pub fn new() -> Self {
        Self {
            assigner: AgentAssigner::new().with_strategy(AssignmentStrategy::LoadBalanced),
            max_parallel: 10,
        }
    }

    pub fn with_max_parallel(mut self, max: usize) -> Self {
        self.max_parallel = max;
        self
    }
}

impl Default for ParallelStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecomposeStrategy for ParallelStrategy {
    fn strategy_type(&self) -> StrategyType {
        StrategyType::Parallel
    }

    #[instrument(skip(self, task))]
    async fn analyze(&self, task: &Task) -> Result<TaskAnalysis> {
        debug!(task_id = %task.id, "Analyzing task for parallel execution");

        let mut analysis = TaskAnalyzer::analyze(task)?;

        // 并行策略：任务可以同时执行
        analysis.suggested_strategy = StrategyType::Parallel;
        analysis.can_parallelize = true;

        // 清除依赖，因为并行执行不依赖顺序
        analysis.dependencies.clear();

        // 估算并行执行时间
        if analysis.estimated_steps > 0 {
            let parallel_groups = (analysis.estimated_steps + self.max_parallel - 1) / self.max_parallel;
            analysis.estimated_duration = Some(Duration::from_secs(parallel_groups as u64));
        }

        info!(
            task_id = %task.id,
            steps = analysis.estimated_steps,
            max_parallel = self.max_parallel,
            "Task analysis complete for parallel strategy"
        );

        Ok(analysis)
    }

    #[instrument(skip(self, analysis))]
    async fn generate(&self, analysis: &TaskAnalysis) -> Result<Vec<SubTask>> {
        debug!(
            task_id = %analysis.task_id,
            steps = analysis.estimated_steps,
            "Generating parallel subtasks"
        );

        let mut subtasks = Vec::with_capacity(analysis.estimated_steps);

        for i in 0..analysis.estimated_steps {
            let subtask = SubTask {
                id: format!("{}_par_{}", analysis.task_id, Uuid::new_v4()),
                parent_id: analysis.task_id.clone(),
                description: format!("Parallel task {}", i + 1),
                parameters: serde_json::json!({
                    "parallel_index": i,
                    "total_parallel": analysis.estimated_steps,
                    "can_run_in_parallel": true,
                }),
                assigned_to: None,
            };

            debug!(subtask_id = %subtask.id, parallel_idx = i, "Created parallel subtask");
            subtasks.push(subtask);
        }

        info!(
            task_id = %analysis.task_id,
            subtask_count = subtasks.len(),
            max_concurrent = self.max_parallel,
            "Generated parallel subtasks"
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
            "Assigning parallel subtasks using load balancing"
        );

        // 并行执行时使用负载均衡
        self.assigner.assign(subtasks, agents)?;

        // 统计每个 agent 的分配数量
        let mut assignment_count: HashMap<String, usize> = HashMap::new();

        for subtask in subtasks.iter() {
            if let Some(ref agent_id) = subtask.assigned_to {
                *assignment_count.entry(agent_id.clone()).or_insert(0) += 1;
            }
        }

        info!(
            assignments = ?assignment_count,
            "Parallel assignment complete (load-balanced)"
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
            id: "parallel-task".to_string(),
            description: "Test parallel task".to_string(),
            context: json!({
                "steps": [
                    {"name": "task1"},
                    {"name": "task2"},
                    {"name": "task3"},
                    {"name": "task4"}
                ]
            }),
        }
    }

    fn create_test_agents() -> Vec<AgentInfo> {
        vec![
            AgentInfo::new("agent-1", "Worker 1").with_max_load(5),
            AgentInfo::new("agent-2", "Worker 2").with_max_load(5),
            AgentInfo::new("agent-3", "Worker 3").with_max_load(5),
        ]
    }

    #[tokio::test]
    async fn test_parallel_analysis() {
        let strategy = ParallelStrategy::new();
        let task = create_test_task();

        let analysis = strategy.analyze(&task).await.unwrap();
        assert_eq!(analysis.suggested_strategy, StrategyType::Parallel);
        assert!(analysis.can_parallelize);
        assert!(analysis.dependencies.is_empty());
    }

    #[tokio::test]
    async fn test_parallel_generate() {
        let strategy = ParallelStrategy::new();
        let task = create_test_task();

        let analysis = strategy.analyze(&task).await.unwrap();
        let subtasks = strategy.generate(&analysis).await.unwrap();

        assert_eq!(subtasks.len(), 4);

        // 所有子任务都标记为可并行
        for subtask in &subtasks {
            assert!(subtask.parameters["can_run_in_parallel"]
                .as_bool()
                .unwrap());
        }
    }

    #[tokio::test]
    async fn test_parallel_assign_load_balanced() {
        let strategy = ParallelStrategy::new().with_max_parallel(2);
        let task = create_test_task();
        let agents = create_test_agents();

        let result = strategy.decompose(&task, &agents).await.unwrap();

        assert_eq!(result.subtasks.len(), 4);
        assert_eq!(result.strategy_used, StrategyType::Parallel);

        // 验证负载均衡
        let mut load: HashMap<String, usize> = HashMap::new();
        for subtask in &result.subtasks {
            if let Some(ref agent_id) = subtask.assigned_to {
                *load.entry(agent_id.clone()).or_insert(0) += 1;
            }
        }

        // 负载应该相对均衡
        let loads: Vec<usize> = load.values().cloned().collect();
        let max_load = *loads.iter().max().unwrap_or(&0);
        let min_load = *loads.iter().min().unwrap_or(&0);
        assert!(max_load - min_load <= 1, "Load should be balanced");
    }
}
