//! Coordinator Agent Module
//!
//! Provides task orchestration, decomposition, and multi-agent coordination.

mod coordinator;
mod result;
mod specialized;

pub use coordinator::{Coordinator, CoordinatorConfig, ExecutionResult};
pub use result::{ParallelExecutionResult, SubtaskResult};
pub use specialized::{
    CodeAnalysisAgent, CodePerformanceAgent, CodeSecurityAgent,
    CodeStyleAgent, CodeStructureAgent, SpecializedAgent,
};
