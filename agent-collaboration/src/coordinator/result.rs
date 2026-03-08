//! Coordinator Result Types
//!
//! Provides types for tracking partial failures in parallel execution.

use crate::agent::Output;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of executing a single subtask
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskResult {
    /// The subtask ID
    pub subtask_id: String,
    /// The agent that was assigned to this subtask
    pub agent_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Output if succeeded
    pub output: Option<Output>,
}

impl SubtaskResult {
    pub fn success(subtask_id: impl Into<String>, agent_id: impl Into<String>, output: Output) -> Self {
        Self {
            subtask_id: subtask_id.into(),
            agent_id: agent_id.into(),
            success: true,
            error_message: None,
            output: Some(output),
        }
    }

    pub fn failure(subtask_id: impl Into<String>, agent_id: impl Into<String>, error_message: String) -> Self {
        Self {
            subtask_id: subtask_id.into(),
            agent_id: agent_id.into(),
            success: false,
            error_message: Some(error_message),
            output: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn is_failure(&self) -> bool {
        !self.success
    }
}

/// Aggregated results from parallel execution
#[derive(Debug)]
pub struct ParallelExecutionResult {
    /// Successfully completed subtask outputs, keyed by subtask ID
    pub successful: HashMap<String, Output>,
    /// Failed subtask results with error details
    pub failed: Vec<SubtaskResult>,
    /// Total number of subtasks
    pub total: usize,
    /// Number of successful executions
    pub success_count: usize,
    /// Number of failed executions
    pub failure_count: usize,
}

impl ParallelExecutionResult {
    pub fn new() -> Self {
        Self {
            successful: HashMap::new(),
            failed: Vec::new(),
            total: 0,
            success_count: 0,
            failure_count: 0,
        }
    }

    pub fn with_capacity(total: usize) -> Self {
        Self {
            successful: HashMap::with_capacity(total),
            failed: Vec::new(),
            total,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Add a successful result
    pub fn add_success(&mut self, subtask_id: String, output: Output) {
        self.successful.insert(subtask_id, output);
        self.success_count += 1;
    }

    /// Add a failed result
    pub fn add_failure(&mut self, result: SubtaskResult) {
        self.failed.push(result);
        self.failure_count += 1;
    }

    /// Check if all subtasks succeeded
    pub fn is_complete_success(&self) -> bool {
        self.failure_count == 0 && self.total > 0
    }

    /// Check if some succeeded and some failed
    pub fn is_partial_success(&self) -> bool {
        self.success_count > 0 && self.failure_count > 0
    }

    /// Check if all subtasks failed
    pub fn is_complete_failure(&self) -> bool {
        self.success_count == 0 && self.total > 0
    }

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.success_count as f64 / self.total as f64) * 100.0
        }
    }

    /// Merge results from a vector of SubtaskResult
    pub fn from_results(results: Vec<SubtaskResult>) -> Self {
        let total = results.len();
        let mut execution_result = Self::with_capacity(total);

        for result in results {
            execution_result.total += 1;
            if result.is_success() {
                if let Some(output) = result.output.clone() {
                    execution_result.add_success(result.subtask_id.clone(), output);
                }
            } else {
                execution_result.add_failure(result);
            }
        }

        execution_result
    }
}

impl Default for ParallelExecutionResult {
    fn default() -> Self {
        Self::new()
    }
}
