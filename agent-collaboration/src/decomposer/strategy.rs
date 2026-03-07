use crate::agent::{SubTask, Task};
use crate::error::Result;
use crate::decomposer::{AgentInfo, TaskAnalysis};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyType {
    Sequential,
    Parallel,
    Hybrid,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyType::Sequential => write!(f, "Sequential"),
            StrategyType::Parallel => write!(f, "Parallel"),
            StrategyType::Hybrid => write!(f, "Hybrid"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompositionResult {
    pub subtasks: Vec<SubTask>,
    pub strategy_used: StrategyType,
    pub estimated_duration: Option<Duration>,
}

#[async_trait]
pub trait DecomposeStrategy: Send + Sync {
    fn strategy_type(&self) -> StrategyType;

    async fn analyze(&self, task: &Task) -> Result<TaskAnalysis>;

    async fn generate(&self, analysis: &TaskAnalysis) -> Result<Vec<SubTask>>;

    async fn assign(
        &self,
        subtasks: &mut [SubTask],
        agents: &[AgentInfo],
    ) -> Result<()>;

    async fn decompose(
        &self,
        task: &Task,
        agents: &[AgentInfo],
    ) -> Result<DecompositionResult> {
        let analysis = self.analyze(task).await?;
        let mut subtasks = self.generate(&analysis).await?;
        self.assign(&mut subtasks, agents).await?;

        Ok(DecompositionResult {
            subtasks,
            strategy_used: self.strategy_type(),
            estimated_duration: analysis.estimated_duration,
        })
    }
}
