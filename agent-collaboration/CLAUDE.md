# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test

```bash
cargo build     # Build the project
cargo test      # Run all tests (38 tests)
cargo check     # Quick type check
```

## Architecture

A Rust Agent framework for multi-agent collaboration with async support.

### Core Modules

- **`src/agent/`** - Agent trait and BaseAgent implementation
  - `base.rs`: Agent trait with 4 core methods (understand, decompose, execute, communicate)
  - `state.rs`: StateMachine with state history, error message separation, transition rules

- **`src/capability/`** - Hybrid capability system
  - `trait.rs`: Capability trait for basic capabilities
  - `registry.rs`: Dynamic capability registry for extended capabilities

- **`src/communication/`** - Bidirectional messaging
  - `message.rs`: Message types (Task, Query, Response, Event)
  - `channel.rs`: Async channels using tokio::sync::mpsc

- **`src/error.rs`** - Enhanced error types with category, recovery suggestions
- **`src/logging.rs`** - Tracing-based logging with configurable output

### Key Patterns

- All async methods use `async_trait`
- Agent state transitions are validated by StateMachine
- Capabilities are registered via `Arc<dyn Capability>`
- Logging via `tracing` crate with `#[instrument]` macros
- Error classification: Transient, Permanent, Configuration, Validation, Timeout
- Task decomposition uses `DecomposeStrategy` trait with 3 strategies

### Task Decomposer

The decomposer module (`src/decomposer/`) supports three decomposition strategies:

| Strategy | Use Case | Description |
|----------|---------|-------------|
| `SequentialStrategy` | Tasks with strict dependencies | Steps execute one after another |
| `ParallelStrategy` | Independent tasks | All subtasks execute simultaneously |
| `HybridStrategy` | Mixed dependencies | Combines sequential and parallel execution |

**Key Types:**
- `TaskAnalysis` - Complexity assessment, dependency analysis, strategy suggestion
- `AgentInfo` - Agent capabilities and load information
- `ExecutionPlan` - Stage-based execution plan for hybrid strategy

**Usage:**
```rust
let strategy = SequentialStrategy::new();
let analysis = strategy.analyze(&task).await?;
let subtasks = strategy.generate(&analysis).await?;
strategy.assign(&mut subtasks, &agents).await?;
```

### Logging

Initialize logging before using agents:
```rust
agent_collaboration::init_logging();
// Or with custom config:
agent_collaboration::init_logging_with_config(
    agent_collaboration::LoggingConfig::new()
        .with_filter("debug,agent_collaboration=trace")
);
```

### State Management

- `AgentState::Failed` no longer contains String (fixed Hash issue)
- Error messages stored separately in `StateMachine::error_message`
- State history available via `StateMachine::history()`
- Use `fail()` method to set Failed state with error message
