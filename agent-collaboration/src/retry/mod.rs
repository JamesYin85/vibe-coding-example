//! Retry mechanism with configurable policies
//!
//! This module provides retry functionality with:
//! - Configurable retry policies (max attempts, backoff strategies)
//! - Circuit breaker pattern for resilience
//! - Retry budget management
//! - Per-error-type retry decisions

pub mod policy;
pub mod circuit_breaker;
pub mod executor;

pub use policy::{RetryPolicy, BackoffStrategy, RetryConfig};
pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use executor::{RetryExecutor, RetryResult};
