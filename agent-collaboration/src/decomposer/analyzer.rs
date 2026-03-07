use crate::agent::Task;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Complexity {
    Simple,     // 单步骤，无需分解
    Medium,     // 2-5步骤，简单依赖
    Complex,    // 多步骤，复杂依赖
}

impl Default for Complexity {
    fn default() -> Self {
        Self::Simple
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyType {
    Data,       // 数据依赖：需要前一步的输出
    Resource,   // 资源依赖：需要共享资源
    Temporal,   // 时间依赖：必须按时间顺序
    Conditional,// 条件依赖：根据条件决定
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub from_step: usize,
    pub to_step: usize,
    pub dep_type: DependencyType,
    pub description: Option<String>,
}

impl Dependency {
    pub fn new(from: usize, to: usize, dep_type: DependencyType) -> Self {
        Self {
            from_step: from,
            to_step: to,
            dep_type,
            description: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    pub task_id: String,
    pub complexity: Complexity,
    pub dependencies: Vec<Dependency>,
    pub required_capabilities: Vec<String>,
    pub estimated_steps: usize,
    pub estimated_duration: Option<Duration>,
    pub can_parallelize: bool,
    pub suggested_strategy: super::StrategyType,
}

impl TaskAnalysis {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            complexity: Complexity::Simple,
            dependencies: Vec::new(),
            required_capabilities: Vec::new(),
            estimated_steps: 1,
            estimated_duration: None,
            can_parallelize: false,
            suggested_strategy: super::StrategyType::Sequential,
        }
    }

    pub fn with_complexity(mut self, complexity: Complexity) -> Self {
        self.complexity = complexity;
        self
    }

    pub fn with_dependency(mut self, dep: Dependency) -> Self {
        self.dependencies.push(dep);
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capabilities.push(cap.into());
        self
    }

    pub fn with_estimated_steps(mut self, steps: usize) -> Self {
        self.estimated_steps = steps;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.estimated_duration = Some(duration);
        self
    }

    pub fn can_parallelize(mut self, yes: bool) -> Self {
        self.can_parallelize = yes;
        self
    }

    pub fn suggest_strategy(mut self, strategy: super::StrategyType) -> Self {
        self.suggested_strategy = strategy;
        self
    }
}

pub struct TaskAnalyzer;

impl TaskAnalyzer {
    pub fn analyze(task: &Task) -> Result<TaskAnalysis> {
        debug!(task_id = %task.id, "Analyzing task");

        let mut analysis = TaskAnalysis::new(&task.id);

        // 基于 context 分析任务复杂度
        if let Some(steps) = task.context.get("steps").and_then(|s| s.as_array()) {
            analysis.estimated_steps = steps.len();

            // 解析依赖关系
            for (i, step) in steps.iter().enumerate() {
                if let Some(deps) = step.get("depends_on").and_then(|d| d.as_array()) {
                    for dep_idx in deps.iter().filter_map(|d| d.as_u64()) {
                        if (dep_idx as usize) < i {
                            analysis.dependencies.push(Dependency::new(
                                dep_idx as usize,
                                i,
                                DependencyType::Data,
                            ));
                        }
                    }
                }
            }
        }

        // 分析所需能力
        if let Some(caps) = task.context.get("required_capabilities").and_then(|c| c.as_array()) {
            for cap in caps.iter().filter_map(|c| c.as_str()) {
                analysis.required_capabilities.push(cap.to_string());
            }
        }

        // 确定复杂度和策略建议
        analysis = Self::determine_complexity(analysis);

        Ok(analysis)
    }

    fn determine_complexity(mut analysis: TaskAnalysis) -> TaskAnalysis {
        match analysis.estimated_steps {
            0..=1 => {
                analysis.complexity = Complexity::Simple;
                analysis.can_parallelize = false;
                analysis.suggested_strategy = super::StrategyType::Sequential;
            }
            2..=5 => {
                if analysis.dependencies.is_empty() {
                    analysis.complexity = Complexity::Medium;
                    analysis.can_parallelize = true;
                    analysis.suggested_strategy = super::StrategyType::Parallel;
                } else {
                    analysis.complexity = Complexity::Medium;
                    analysis.can_parallelize = false;
                    analysis.suggested_strategy = super::StrategyType::Sequential;
                }
            }
            _ => {
                analysis.complexity = Complexity::Complex;
                analysis.can_parallelize = analysis.dependencies.len() < analysis.estimated_steps / 2;
                analysis.suggested_strategy = if analysis.can_parallelize {
                    super::StrategyType::Hybrid
                } else {
                    super::StrategyType::Sequential
                };
            }
        }

        analysis
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_analyze_simple_task() {
        let task = Task {
            id: "task-1".to_string(),
            description: "Simple task".to_string(),
            context: json!({}),
        };

        let analysis = TaskAnalyzer::analyze(&task).unwrap();
        assert_eq!(analysis.complexity, Complexity::Simple);
        assert_eq!(analysis.estimated_steps, 1);
    }

    #[test]
    fn test_analyze_task_with_steps() {
        let task = Task {
            id: "task-2".to_string(),
            description: "Multi-step task".to_string(),
            context: json!({
                "steps": [
                    {"name": "step1"},
                    {"name": "step2", "depends_on": [0]},
                    {"name": "step3", "depends_on": [1]}
                ]
            }),
        };

        let analysis = TaskAnalyzer::analyze(&task).unwrap();
        assert_eq!(analysis.estimated_steps, 3);
        assert_eq!(analysis.dependencies.len(), 2);
    }

    #[test]
    fn test_dependency_creation() {
        let dep = Dependency::new(0, 1, DependencyType::Data)
            .with_description("Need output from step 0");

        assert_eq!(dep.from_step, 0);
        assert_eq!(dep.to_step, 1);
        assert!(dep.description.is_some());
    }
}
