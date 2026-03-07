pub mod agent;
pub mod capability;
pub mod communication;
pub mod coordinator;
pub mod decomposer;
pub mod error;
pub mod llm;
pub mod logging;
pub mod retry;

pub use agent::{Agent, BaseAgent, AgentState, SubTask, Task, Output};
pub use capability::{Capability, CapabilityRegistry};
pub use communication::{Message, Channel};
pub use coordinator::{
    Coordinator, CoordinatorConfig, ExecutionResult,
    CodeAnalysisAgent, CodePerformanceAgent, CodeSecurityAgent,
    CodeStyleAgent, CodeStructureAgent, SpecializedAgent,
};
pub use decomposer::{
    AgentAssigner, AgentInfo, AssignmentStrategy, Complexity, DecomposeStrategy,
    DecompositionResult, Dependency, DependencyType, ExecutionPlan, ExecutionStage,
    HybridStrategy, ParallelStrategy, SequentialStrategy, StrategyType, TaskAnalysis,
};
pub use error::{AgentError, Result};
pub use llm::{
    AnthropicClient, CompletionRequest, CompletionResponse, FallbackClient,
    FallbackClientBuilder, FallbackConfig, LLMClient, LLMConfig, LLMError, LLMResult,
    Message as LLMMessage, OpenAIClient, Provider, Usage,
};
pub use logging::{init_logging, init_logging_with_config, LoggingConfig};
