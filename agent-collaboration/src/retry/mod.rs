//! Retry mechanism with configurable policies and circuit breaker
//!
//! This module provides comprehensive retry functionality for building resilient
//! distributed systems. It includes:
//!
//! - **Retry policies** with configurable backoff strategies
//! - **Circuit breaker** pattern to prevent cascading failures
//! - **Retry executor** for async operations with automatic retries
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use agent_collaboration::retry::{RetryExecutor, RetryExecutorBuilder, RetryResult};
//! use agent_collaboration::error::AgentError;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), AgentError> {
//!     let executor = RetryExecutorBuilder::new()
//!         .max_retries(3)
//!         .exponential_backoff()
//!         .build();
//!
//!     let result = executor
//!         .execute("fetch_data", || async {
//!             // Your async operation here
//!             Ok::<_, AgentError>(42)
//!         })
//!         .await;
//!
//!     match result {
//!         RetryResult::Success(value) => println!("Got: {}", value),
//!         RetryResult::Failed { error, .. } => eprintln!("Failed: {}", error),
//!         RetryResult::Cancelled => println!("Cancelled"),
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Backoff Strategies
//!
//! The module supports multiple backoff strategies:
//!
//! - **Fixed**: Constant delay between retries
//! - **Linear**: Delay increases linearly (delay * attempt)
//! - **Exponential**: Delay doubles each attempt (delay * 2^attempt)
//! - **ExponentialWithJitter**: Exponential with random jitter to avoid thundering herd
//!
//! # Circuit Breaker
//!
//! Use the circuit breaker to prevent cascading failures:
//!
//! ```rust,ignore
//! use agent_collaboration::retry::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
//!
//! let circuit_breaker = CircuitBreaker::new(CircuitBreakerConfig {
//!     failure_threshold: 5,
//!     failure_window_secs: 60,
//!     reset_timeout_secs: 30,
//!     half_open_max_calls: 3,
//!     half_open_timeout_secs: 10,
//! });
//!
//! // Check before making calls
//! if circuit_breaker.is_call_allowed() {
//!     match some_operation() {
//!         Ok(result) => circuit_breaker.record_success(),
//!         Err(e) => circuit_breaker.record_failure(),
//!     }
//! }
//! ```

pub mod policy;
pub mod circuit_breaker;
pub mod executor;

pub use policy::{RetryPolicy, BackoffStrategy, RetryConfig};
pub use circuit_breaker::{CircuitBreaker, CircuitState, CircuitBreakerConfig};
pub use executor::{RetryExecutor, RetryResult, RetryExecutorBuilder};
