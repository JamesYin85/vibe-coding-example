use crate::agent::{SubTask, Task};
use crate::decomposer::{
    AgentAssigner, AgentInfo, AssignmentStrategy, Complexity, DecomposeStrategy, Dependency,
    DependencyType, StrategyType, TaskAnalysis, TaskAnalyzer,
};
use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStage {
    pub stage_id: usize,
    pub subtask_ids: Vec<String>,
    pub can_parallelize: bool,
    pub depends_on_stages: Vec<usize>,
}

impl ExecutionStage {
    pub fn new(stage_id: usize) -> Self {
        Self {
            stage_id,
            subtask_ids: Vec::new(),
            can_parallelize: true,
            depends_on_stages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub stages: Vec<ExecutionStage>,
    pub total_stages: usize,
    pub critical_path_length: usize,
}

impl ExecutionPlan {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            total_stages: 0,
            critical_path_length: 0,
        }
    }

    pub fn add_stage(&mut self, stage: ExecutionStage) {
        self.total_stages += 1;
        self.stages.push(stage);
    }
}

impl Default for ExecutionPlan {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HybridStrategy {
    assigner: AgentAssigner,
}

impl HybridStrategy {
    pub fn new() -> Self {
        Self {
            assigner: AgentAssigner::new().with_strategy(AssignmentStrategy::LoadBalanced),
        }
    }

    fn build_execution_plan(
        &self,
        subtasks: &[SubTask],
        dependencies: &[Dependency],
    ) -> ExecutionPlan {
        let mut plan = ExecutionPlan::new();

        if subtasks.is_empty() {
            return plan;
        }

        // 构建依赖图
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        let mut dependents: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut depends_on: HashMap<usize, Vec<usize>> = HashMap::new();

        for (i, _) in subtasks.iter().enumerate() {
            in_degree.insert(i, 0);
            dependents.insert(i, Vec::new());
            depends_on.insert(i, Vec::new());
        }

        for dep in dependencies {
            *in_degree.get_mut(&dep.to_step).unwrap() += 1;
            dependents.get_mut(&dep.from_step).unwrap().push(dep.to_step);
            depends_on.get_mut(&dep.to_step).unwrap().push(dep.from_step);
        }

        // 拓扑排序，将同层任务归为一个 stage
        let mut remaining: HashSet<usize> = (0..subtasks.len()).collect();
        let mut stage_id = 0;

        while !remaining.is_empty() {
            // 找出所有入度为0的任务
            let ready: Vec<usize> = remaining
                .iter()
                .filter(|&&i| in_degree.get(&i).copied().unwrap_or(0) == 0)
                .copied()
                .collect();

            if ready.is_empty() {
                warn!("Circular dependency detected in task graph");
                break;
            }

            let mut stage = ExecutionStage::new(stage_id);
            stage.can_parallelize = ready.len() > 1;

            // 计算此 stage 依赖的 stages
            for &task_idx in &ready {
                for &dep_idx in depends_on.get(&task_idx).unwrap() {
                    // 找到 dep_idx 所在的 stage
                    for (prev_stage_id, prev_stage) in plan.stages.iter().enumerate() {
                        if prev_stage.subtask_ids.contains(&subtasks[dep_idx].id) {
                            if !stage.depends_on_stages.contains(&prev_stage_id) {
                                stage.depends_on_stages.push(prev_stage_id);
                            }
                        }
                    }
                }
            }

            for &task_idx in &ready {
                stage.subtask_ids.push(subtasks[task_idx].id.clone());
                remaining.remove(&task_idx);

                // 更新依赖此任务的其他任务的入度
                for &dependent in dependents.get(&task_idx).unwrap() {
                    if let Some(deg) = in_degree.get_mut(&dependent) {
                        *deg = deg.saturating_sub(1);
                    }
                }
            }

            plan.add_stage(stage);
            stage_id += 1;
        }

        plan.critical_path_length = plan.stages.len();
        plan
    }
}

impl Default for HybridStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecomposeStrategy for HybridStrategy {
    fn strategy_type(&self) -> StrategyType {
        StrategyType::Hybrid
    }

    #[instrument(skip(self, task))]
    async fn analyze(&self, task: &Task) -> Result<TaskAnalysis> {
        debug!(task_id = %task.id, "Analyzing task for hybrid execution");

        let mut analysis = TaskAnalyzer::analyze(task)?;

        // 混合策略：根据依赖关系决定并行度
        analysis.suggested_strategy = StrategyType::Hybrid;

        // 有依赖但也可以部分并行
        analysis.can_parallelize =
            analysis.complexity == Complexity::Complex || analysis.complexity == Complexity::Medium;

        // 估算执行时间基于关键路径
        if analysis.estimated_steps > 0 {
            let parallel_factor = if analysis.dependencies.is_empty() {
                analysis.estimated_steps
            } else {
                (analysis.dependencies.len() + 1).max(1)
            };
            analysis.estimated_duration = Some(Duration::from_secs(
                (analysis.estimated_steps / parallel_factor.max(1)) as u64,
            ));
        }

        info!(
            task_id = %task.id,
            complexity = ?analysis.complexity,
            steps = analysis.estimated_steps,
            dependencies = analysis.dependencies.len(),
            "Task analysis complete for hybrid strategy"
        );

        Ok(analysis)
    }

    #[instrument(skip(self, analysis))]
    async fn generate(&self, analysis: &TaskAnalysis) -> Result<Vec<SubTask>> {
        debug!(
            task_id = %analysis.task_id,
            steps = analysis.estimated_steps,
            deps = analysis.dependencies.len(),
            "Generating hybrid subtasks"
        );

        let mut subtasks = Vec::with_capacity(analysis.estimated_steps);

        for i in 0..analysis.estimated_steps {
            // 找出此步骤的依赖
            let step_deps: Vec<usize> = analysis
                .dependencies
                .iter()
                .filter(|d| d.to_step == i)
                .map(|d| d.from_step)
                .collect();

            let subtask = SubTask {
                id: format!("{}_hyb_{}", analysis.task_id, Uuid::new_v4()),
                parent_id: analysis.task_id.clone(),
                description: format!("Hybrid task {}", i + 1),
                parameters: serde_json::json!({
                    "step_index": i,
                    "total_steps": analysis.estimated_steps,
                    "dependencies": step_deps,
                    "dependency_count": step_deps.len(),
                }),
                assigned_to: None,
            };

            debug!(
                subtask_id = %subtask.id,
                step = i,
                deps = step_deps.len(),
                "Created hybrid subtask"
            );
            subtasks.push(subtask);
        }

        info!(
            task_id = %analysis.task_id,
            subtask_count = subtasks.len(),
            "Generated hybrid subtasks"
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
            "Assigning hybrid subtasks"
        );

        // 从 subtask 参数中重建依赖关系
        let mut dependencies = Vec::new();
        for (i, subtask) in subtasks.iter().enumerate() {
            if let Some(deps) = subtask.parameters.get("dependencies").and_then(|d| d.as_array()) {
                for dep_idx in deps.iter().filter_map(|d| d.as_u64()) {
                    dependencies.push(Dependency::new(dep_idx as usize, i, DependencyType::Data));
                }
            }
        }

        // 构建执行计划
        let plan = self.build_execution_plan(subtasks, &dependencies);

        info!(
            total_stages = plan.total_stages,
            critical_path = plan.critical_path_length,
            "Built execution plan"
        );

        // 根据 stage 信息分配 Agent
        // 同一 stage 内的任务可以并行，使用负载均衡
        self.assigner.assign(subtasks, agents)?;

        // 将执行计划信息添加到每个 subtask
        for stage in &plan.stages {
            for subtask in subtasks.iter_mut() {
                if stage.subtask_ids.contains(&subtask.id) {
                    subtask.parameters["stage_id"] = serde_json::json!(stage.stage_id);
                    subtask.parameters["can_parallelize"] = serde_json::json!(stage.can_parallelize);
                    subtask.parameters["depends_on_stages"] =
                        serde_json::json!(stage.depends_on_stages);
                }
            }
        }

        info!(
            stages = plan.total_stages,
            "Hybrid assignment complete"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_task_with_deps() -> Task {
        Task {
            id: "hybrid-task".to_string(),
            description: "Test hybrid task".to_string(),
            context: json!({
                "steps": [
                    {"name": "step1"},
                    {"name": "step2", "depends_on": [0]},
                    {"name": "step3", "depends_on": [0]},
                    {"name": "step4", "depends_on": [1, 2]}
                ]
            }),
        }
    }

    fn create_test_agents() -> Vec<AgentInfo> {
        vec![
            AgentInfo::new("agent-1", "Worker 1").with_max_load(5),
            AgentInfo::new("agent-2", "Worker 2").with_max_load(5),
        ]
    }

    #[tokio::test]
    async fn test_hybrid_analysis() {
        let strategy = HybridStrategy::new();
        let task = create_test_task_with_deps();

        let analysis = strategy.analyze(&task).await.unwrap();
        assert_eq!(analysis.suggested_strategy, StrategyType::Hybrid);
        assert_eq!(analysis.estimated_steps, 4);
        assert_eq!(analysis.dependencies.len(), 4); // 4 dependency edges: 0->1, 0->2, 1->3, 2->3
    }

    #[tokio::test]
    async fn test_hybrid_generate() {
        let strategy = HybridStrategy::new();
        let task = create_test_task_with_deps();

        let analysis = strategy.analyze(&task).await.unwrap();
        let subtasks = strategy.generate(&analysis).await.unwrap();

        assert_eq!(subtasks.len(), 4);

        // 验证依赖信息被记录
        assert!(subtasks[1].parameters["dependencies"]
            .as_array()
            .unwrap()
            .contains(&json!(0)));
        assert!(subtasks[2].parameters["dependencies"]
            .as_array()
            .unwrap()
            .contains(&json!(0)));
    }

    #[tokio::test]
    async fn test_execution_plan_building() {
        let strategy = HybridStrategy::new();

        let subtasks = vec![
            SubTask {
                id: "s1".to_string(),
                parent_id: "p".to_string(),
                description: "".to_string(),
                parameters: json!({"dependencies": []}),
                assigned_to: None,
            },
            SubTask {
                id: "s2".to_string(),
                parent_id: "p".to_string(),
                description: "".to_string(),
                parameters: json!({"dependencies": [0]}),
                assigned_to: None,
            },
            SubTask {
                id: "s3".to_string(),
                parent_id: "p".to_string(),
                description: "".to_string(),
                parameters: json!({"dependencies": [0]}),
                assigned_to: None,
            },
        ];

        let deps = vec![
            Dependency::new(0, 1, DependencyType::Data),
            Dependency::new(0, 2, DependencyType::Data),
        ];

        let plan = strategy.build_execution_plan(&subtasks, &deps);

        assert_eq!(plan.total_stages, 2);
        assert_eq!(plan.stages[0].subtask_ids.len(), 1);
        assert_eq!(plan.stages[1].subtask_ids.len(), 2);
        assert!(plan.stages[1].can_parallelize);
    }

    #[tokio::test]
    async fn test_hybrid_full_decompose() {
        let strategy = HybridStrategy::new();
        let task = create_test_task_with_deps();
        let agents = create_test_agents();

        let result = strategy.decompose(&task, &agents).await.unwrap();

        assert_eq!(result.subtasks.len(), 4);
        assert_eq!(result.strategy_used, StrategyType::Hybrid);

        // 验证所有子任务都被分配了 stage
        for subtask in &result.subtasks {
            assert!(subtask.parameters.get("stage_id").is_some());
        }
    }
}
