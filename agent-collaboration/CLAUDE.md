# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test

```bash
cargo build     # Build the project
cargo test      # Run all tests (55 tests)
cargo check     # Quick type check
cargo doc --open # Generate and open documentation
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
- **`src/retry/`** - Retry mechanism with circuit breaker
  - `policy.rs`: RetryConfig, RetryPolicy, BackoffStrategy
  - `circuit_breaker.rs`: CircuitBreaker with Closed/Open/HalfOpen states
  - `executor.rs`: Async RetryExecutor with timeout support
- **`src/llm/`** - LLM interface layer
  - `client.rs`: LLMClient trait
  - `openai.rs`, `anthropic.rs`: Provider implementations
  - `fallback.rs`: Fallback mechanism with retry
- **`src/coordinator/`** - Multi-agent coordination
  - `coordinator.rs`: Orchestrates agent collaboration
  - Specialized agents: CodeStyleAgent, CodeSecurityAgent, CodePerformanceAgent, CodeStructureAgent

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

### Retry Mechanism

The retry module (`src/retry/`) provides resilient operation execution:

**Backoff Strategies:**
| Strategy | Formula | Use Case |
|----------|---------|----------|
| `Fixed` | delay | Simple scenarios |
| `Linear` | delay * attempt | Gradual increase |
| `Exponential` | delay * 2^attempt | Distributed systems |
| `ExponentialWithJitter` | exponential ± 20% | Avoid thundering herd |

**Circuit Breaker States:**
- `Closed`: Normal operation, failures counted
- `Open`: Requests blocked, waiting for reset timeout
- `HalfOpen`: Limited requests allowed to test recovery

**Usage:**
```rust
let executor = RetryExecutorBuilder::new()
    .max_retries(3)
    .exponential_with_jitter()
    .attempt_timeout(5000)
    .build();

let result = executor.execute("operation", || async {
    some_fallible_operation().await
}).await;
```

### LLM Interface

The LLM module (`src/llm/`) provides a unified interface for multiple providers:

```rust
let client = FallbackClientBuilder::new()
    .primary(OpenAIClient::new(config))
    .secondary(AnthropicClient::new(config))
    .build();

let response = client.complete(request).await?;
```

### Coordinator Agent

The coordinator orchestrates multi-agent collaboration:

```rust
let coordinator = Coordinator::new(config);
let result = coordinator.analyze_code(&code).await?;
// Uses specialized agents: Style, Security, Performance, Structure
```
