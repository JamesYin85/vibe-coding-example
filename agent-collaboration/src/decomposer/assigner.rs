use crate::agent::SubTask;
use crate::error::{AgentError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, trace};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentStrategy {
    RoundRobin,       // 轮询分配
    CapabilityBased,  // 按能力匹配
    LoadBalanced,     // 负载均衡
}

impl Default for AssignmentStrategy {
    fn default() -> Self {
        Self::LoadBalanced
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub current_load: usize,
    #[serde(default)]
    pub max_load: usize,
}

impl AgentInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            capabilities: Vec::new(),
            current_load: 0,
            max_load: 10,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_max_load(mut self, max: usize) -> Self {
        self.max_load = max;
        self
    }

    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    pub fn available_capacity(&self) -> usize {
        self.max_load.saturating_sub(self.current_load)
    }

    pub fn is_available(&self) -> bool {
        self.current_load < self.max_load
    }
}

pub struct AgentAssigner {
    strategy: AssignmentStrategy,
}

impl AgentAssigner {
    pub fn new() -> Self {
        Self {
            strategy: AssignmentStrategy::default(),
        }
    }

    pub fn with_strategy(mut self, strategy: AssignmentStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn assign(
        &self,
        subtasks: &mut [SubTask],
        agents: &[AgentInfo],
    ) -> Result<()> {
        if agents.is_empty() {
            return Err(AgentError::internal("No agents available for assignment"));
        }

        debug!(
            strategy = ?self.strategy,
            subtask_count = subtasks.len(),
            agent_count = agents.len(),
            "Starting agent assignment"
        );

        match self.strategy {
            AssignmentStrategy::RoundRobin => self.assign_round_robin(subtasks, agents),
            AssignmentStrategy::CapabilityBased => self.assign_by_capability(subtasks, agents),
            AssignmentStrategy::LoadBalanced => self.assign_load_balanced(subtasks, agents),
        }
    }

    fn assign_round_robin(&self, subtasks: &mut [SubTask], agents: &[AgentInfo]) -> Result<()> {
        let mut load_tracker: HashMap<String, usize> = HashMap::new();

        for (i, subtask) in subtasks.iter_mut().enumerate() {
            let agent_idx = i % agents.len();
            let agent = &agents[agent_idx];

            *load_tracker.entry(agent.id.clone()).or_insert(0) += 1;
            subtask.assigned_to = Some(agent.id.clone());

            trace!(
                subtask_id = %subtask.id,
                agent_id = %agent.id,
                "Assigned subtask (round-robin)"
            );
        }

        info!(assignments = ?load_tracker, "Round-robin assignment complete");
        Ok(())
    }

    fn assign_by_capability(&self, subtasks: &mut [SubTask], agents: &[AgentInfo]) -> Result<()> {
        for subtask in subtasks.iter_mut() {
            // 查找具有所需能力的 Agent
            let best_agent = agents
                .iter()
                .filter(|a| a.is_available())
                .filter(|a| {
                    // 检查是否有所需能力（基于描述匹配）
                    a.capabilities.iter().any(|cap| {
                        subtask.description.to_lowercase().contains(&cap.to_lowercase())
                    })
                })
                .min_by_key(|a| a.current_load);

            if let Some(agent) = best_agent {
                subtask.assigned_to = Some(agent.id.clone());
                debug!(
                    subtask_id = %subtask.id,
                    agent_id = %agent.id,
                    "Assigned subtask by capability match"
                );
            } else {
                // 回退到第一个可用 Agent
                let fallback = &agents[0];
                subtask.assigned_to = Some(fallback.id.clone());
                debug!(
                    subtask_id = %subtask.id,
                    agent_id = %fallback.id,
                    "Assigned subtask to fallback agent"
                );
            }
        }

        Ok(())
    }

    fn assign_load_balanced(&self, subtasks: &mut [SubTask], agents: &[AgentInfo]) -> Result<()> {
        let mut agent_loads: HashMap<String, usize> = agents
            .iter()
            .map(|a| (a.id.clone(), a.current_load))
            .collect();

        for subtask in subtasks.iter_mut() {
            // 选择负载最低的 Agent
            let best_agent = agents
                .iter()
                .filter(|a| agent_loads.get(&a.id).copied().unwrap_or(0) < a.max_load)
                .min_by_key(|a| agent_loads.get(&a.id).copied().unwrap_or(0));

            if let Some(agent) = best_agent {
                *agent_loads.get_mut(&agent.id).unwrap() += 1;
                subtask.assigned_to = Some(agent.id.clone());

                trace!(
                    subtask_id = %subtask.id,
                    agent_id = %agent.id,
                    new_load = agent_loads.get(&agent.id).unwrap(),
                    "Assigned subtask (load-balanced)"
                );
            } else {
                return Err(AgentError::internal("All agents at maximum load"));
            }
        }

        info!(final_loads = ?agent_loads, "Load-balanced assignment complete");
        Ok(())
    }
}

impl Default for AgentAssigner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_agents() -> Vec<AgentInfo> {
        vec![
            AgentInfo::new("agent-1", "Worker 1")
                .with_capabilities(vec!["data-processing".to_string()])
                .with_max_load(5),
            AgentInfo::new("agent-2", "Worker 2")
                .with_capabilities(vec!["analysis".to_string()])
                .with_max_load(5),
        ]
    }

    fn create_test_subtasks() -> Vec<SubTask> {
        vec![
            SubTask {
                id: "sub-1".to_string(),
                parent_id: "task-1".to_string(),
                description: "data-processing step".to_string(),
                parameters: serde_json::Value::Null,
                assigned_to: None,
            },
            SubTask {
                id: "sub-2".to_string(),
                parent_id: "task-1".to_string(),
                description: "analysis step".to_string(),
                parameters: serde_json::Value::Null,
                assigned_to: None,
            },
        ]
    }

    #[test]
    fn test_agent_info() {
        let agent = AgentInfo::new("a1", "Test Agent")
            .with_capabilities(vec!["cap1".to_string()])
            .with_max_load(5);

        assert!(agent.has_capability("cap1"));
        assert!(!agent.has_capability("cap2"));
        assert!(agent.is_available());
        assert_eq!(agent.available_capacity(), 5);
    }

    #[test]
    fn test_round_robin_assignment() {
        let assigner = AgentAssigner::new()
            .with_strategy(AssignmentStrategy::RoundRobin);

        let agents = create_test_agents();
        let mut subtasks = create_test_subtasks();

        assigner.assign(&mut subtasks, &agents).unwrap();

        assert_eq!(subtasks[0].assigned_to, Some("agent-1".to_string()));
        assert_eq!(subtasks[1].assigned_to, Some("agent-2".to_string()));
    }

    #[test]
    fn test_load_balanced_assignment() {
        let assigner = AgentAssigner::new()
            .with_strategy(AssignmentStrategy::LoadBalanced);

        let agents = create_test_agents();
        let mut subtasks = create_test_subtasks();

        assigner.assign(&mut subtasks, &agents).unwrap();

        // 两个子任务应该分配到负载最低的 agent
        assert!(subtasks[0].assigned_to.is_some());
        assert!(subtasks[1].assigned_to.is_some());
    }
}
