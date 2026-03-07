# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Status

This project contains a multi-agent collaboration framework with Coordinator, specialized agents, task decomposition, and LLM interface layer.

## Build Commands

```bash
cargo build
cargo test
cargo run --example code_analysis_demo
```

## Architecture
```
src/agent/
├── base.rs         # BaseAgent, Agent trait, StateMachine
├── state.rs        # AgentState enum, StateMachine

src/capability/
├── trait.rs         # Capability trait
├── registry.rs     # CapabilityRegistry
src/communication/
├── message.rs      # Message types
├── channel.rs       # Channel for inter-agent communication
src/decomposer/
├── analyzer.rs      # TaskAnalyzer, TaskAnalysis
├── assigner.rs      # AgentAssigner, AgentInfo, AssignmentStrategy
├── strategy.rs      # DecomposeStrategy trait, DecompositionResult
├── sequential.rs  # SequentialStrategy
├── parallel.rs    # ParallelStrategy
├── hybrid.rs        # HybridStrategy (recommended for complex tasks)
src/llm/
├── mod.rs           # Module exports
├── client.rs        # LLMClient trait, CompletionRequest/Response
├── config.rs        # LLMConfig, Provider enum
├── error.rs         # LLMError types with retryable/fallback logic
├── openai.rs         # OpenAI client
├── anthropic.rs     # Anthropic client
├── fallback.rs       # FallbackClient with retry/backoff
src/coordinator/
├── mod.rs           # Module exports
├── coordinator.rs  # Coordinator, CoordinatorConfig, ExecutionResult
├── specialized.rs  # Specialized agents (CodeStyle, Security, Performance, Structure, Analysis)
src/logging.rs           # Logging configuration with tracing
```
## Key Patterns
- **Async-first**: All agents use async/await with tokio
- **Trait-based**: Agent behavior defined via traits with async_trait
- **Builder pattern**: Config types use builder pattern for fluent API
- **Strategy pattern**: Task decomposition uses interchangeable strategies
- **Error recovery**: Comprehensive error types with recovery suggestions
- **Tracing**: Structured logging with tracing crate

## Development
```bash
cargo test                                    # Run all tests
cargo run --example code_analysis_demo  # Run demo
```

## Dependencies
```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.11", features = ["json", "stream"] }
futures = "0.3"
```
## Running Tests
41 tests pass including:
- Agent state management tests
- Capability registration tests
- Communication channel tests
- Task decomposer tests (Sequential, Parallel, Hybrid)
- Coordinator tests
- LLM interface tests

