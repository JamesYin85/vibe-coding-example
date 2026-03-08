//! Retry Executor
//!
//! Provides retry logic for executing async operations with configurable policies.
//!
//! # Example
//!
//! ```rust
//! use agent_collaboration::retry::{RetryExecutorBuilder, RetryResult};
//! use agent_collaboration::error::AgentError;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), AgentError> {
//! let executor = RetryExecutorBuilder::new()
//!     .max_retries(3)
//!     .exponential_with_jitter()
//!     .attempt_timeout(5000)  // 5 second timeout per attempt
//!     .build();
//!
//! let result = executor.execute("fetch_api", || async {
//!     // Your fallible operation here
//!     Ok::<_, AgentError>("success")
//! }).await;
//!
//! match result {
//!     RetryResult::Success(value) => println!("Success: {}", value),
//!     RetryResult::Failed { error, attempts, .. } => {
//!         eprintln!("Failed after {} attempts: {}", attempts, error);
//!     }
//!     RetryResult::Cancelled => println!("Operation cancelled"),
//!     RetryResult::CircuitOpen { .. } => println!("Circuit breaker is open"),
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::AgentError;
use crate::retry::circuit_breaker::CircuitBreaker;
use crate::retry::policy::{BackoffStrategy, RetryConfig, RetryPolicy};
use crate::retry::trait_::RetryableError;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn, instrument};

/// Result of a retry operation
#[derive(Debug)]
pub enum RetryResult<T, E: RetryableError = AgentError> {
    /// Operation succeeded
    Success(T),
    /// Operation failed after all retries
    Failed {
        error: E,
        attempts: u32,
        last_delay_ms: Option<u64>,
    },
    /// Circuit breaker is open, request was blocked
    CircuitOpen {
        error: Option<E>,
    },
    /// Operation was cancelled
    Cancelled,
}

impl<T, E: RetryableError> RetryResult<T, E> {
    pub fn is_success(&self) -> bool {
        matches!(self, RetryResult::Success(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, RetryResult::Failed { .. })
    }

    pub fn is_circuit_open(&self) -> bool {
        matches!(self, RetryResult::CircuitOpen { .. })
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, RetryResult::Cancelled)
    }

    pub fn unwrap(self) -> T {
        match self {
            RetryResult::Success(value) => value,
            RetryResult::Failed { error, .. } => {
                panic!("Called unwrap on a failed result: {}", error.to_error_message())
            }
            RetryResult::CircuitOpen { .. } => panic!("Called unwrap on a circuit open result"),
            RetryResult::Cancelled => panic!("Called unwrap on a cancelled result"),
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            RetryResult::Success(value) => value,
            _ => default,
        }
    }

    /// Map the success value to a new type
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> RetryResult<U, E> {
        match self {
            RetryResult::Success(value) => RetryResult::Success(f(value)),
            RetryResult::Failed { error, attempts, last_delay_ms } => {
                RetryResult::Failed { error, attempts, last_delay_ms }
            }
            RetryResult::CircuitOpen { error } => RetryResult::CircuitOpen { error },
            RetryResult::Cancelled => RetryResult::Cancelled,
        }
    }
}

/// Type alias for backward compatibility with existing code using AgentError
pub type AgentRetryResult<T> = RetryResult<T, AgentError>;

/// Generic retry executor for operations that may fail
pub struct RetryExecutor<E: RetryableError = AgentError> {
    policy: RetryPolicy,
    config: RetryConfig,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    cancellation_token: Option<CancellationToken>,
    _phantom: PhantomData<E>,
}

impl<E: RetryableError> RetryExecutor<E> {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            policy: RetryPolicy::new(config.clone()),
            config,
            circuit_breaker: None,
            cancellation_token: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_policy(policy: RetryPolicy) -> Self {
        Self {
            policy,
            config: RetryConfig::default(),
            circuit_breaker: None,
            cancellation_token: None,
            _phantom: PhantomData,
        }
    }

    /// Check if cancellation has been requested
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.as_ref().map_or(false, |t| t.is_cancelled())
    }

    /// Wait for delay with cancellation support
    async fn wait_with_cancellation(&self, delay: Duration) -> bool {
        if let Some(token) = &self.cancellation_token {
            tokio::select! {
                _ = sleep(delay) => false,
                _ = token.cancelled() => true,
            }
        } else {
            sleep(delay).await;
            false
        }
    }

    /// Check if we should retry based on error
    fn should_retry(&self, error: &E) -> bool {
        error.is_retryable()
            || (self.config.retry_on_timeout && error.is_timeout())
            || (self.config.retry_on_transient && error.is_transient())
    }

    /// Execute an async operation with retry logic
    #[instrument(skip(self, operation), fields(operation_name))]
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> RetryResult<T, E>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
    {
        // Check cancellation at start
        if self.is_cancelled() {
            return RetryResult::Cancelled;
        }

        // Check circuit breaker
        if let Some(cb) = &self.circuit_breaker {
            if !cb.is_call_allowed() {
                warn!(
                    operation_name = %operation_name,
                    "Circuit breaker is open, request blocked"
                );
                return RetryResult::CircuitOpen { error: None };
            }
        }

        let mut attempts = 0;
        let mut last_delay_ms = None;

        loop {
            // Check cancellation before each attempt
            if self.is_cancelled() {
                return RetryResult::Cancelled;
            }

            attempts += 1;

            debug!(
                operation_name = %operation_name,
                attempt = attempts,
                max_retries = self.config.max_retries,
            );

            match operation().await {
                Ok(result) => {
                    info!(
                        operation_name = %operation_name,
                        attempts = attempts,
                        "Operation succeeded"
                    );
                    // Record success to circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_success();
                    }
                    return RetryResult::Success(result);
                }
                Err(error) => {
                    // Record failure to circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_failure();
                    }

                    // Check if we should retry
                    if !self.should_retry(&error) {
                        warn!(
                            operation_name = %operation_name,
                            error = %error.to_error_message(),
                            "Non-retryable error"
                        );
                        return RetryResult::Failed {
                            error,
                            attempts,
                            last_delay_ms,
                        };
                    }

                    // Check if we've exceeded max retries
                    if attempts >= self.config.max_retries {
                        warn!(
                            operation_name = %operation_name,
                            attempts = attempts,
                            error = %error.to_error_message(),
                            "Max retries exceeded"
                        );
                        return RetryResult::Failed {
                            error,
                            attempts,
                            last_delay_ms,
                        };
                    }

                    // Calculate delay
                    let delay = self.policy.config().calculate_delay(attempts);
                    last_delay_ms = Some(delay.as_millis() as u64);

                    info!(
                        operation_name = %operation_name,
                        attempt = attempts,
                        delay_ms = delay.as_millis(),
                        error = %error.to_error_message(),
                        "Retrying after delay"
                    );

                    // Wait with cancellation support
                    if self.wait_with_cancellation(delay).await {
                        return RetryResult::Cancelled;
                    }
                }
            }
        }
    }

    /// Execute with a timeout for each attempt
    #[instrument(skip(self, operation), fields(operation_name))]
    pub async fn execute_with_timeout<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> RetryResult<T, E>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
    {
        if self.config.attempt_timeout_ms == 0 {
            return self.execute(operation_name, operation).await;
        }

        // Check cancellation at start
        if self.is_cancelled() {
            return RetryResult::Cancelled;
        }

        // Check circuit breaker
        if let Some(cb) = &self.circuit_breaker {
            if !cb.is_call_allowed() {
                warn!(
                    operation_name = %operation_name,
                    "Circuit breaker is open, request blocked"
                );
                return RetryResult::CircuitOpen { error: None };
            }
        }

        let timeout = Duration::from_millis(self.config.attempt_timeout_ms);
        let mut attempts = 0;
        let mut last_delay_ms = None;

        loop {
            // Check cancellation before each attempt
            if self.is_cancelled() {
                return RetryResult::Cancelled;
            }

            attempts += 1;

            debug!(
                operation_name = %operation_name,
                attempt = attempts,
                max_retries = self.config.max_retries,
                timeout_ms = self.config.attempt_timeout_ms
            );

            match tokio::time::timeout(timeout, operation()).await {
                Ok(Ok(result)) => {
                    info!(
                        operation_name = %operation_name,
                        attempts = attempts,
                        "Operation succeeded"
                    );
                    // Record success to circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_success();
                    }
                    return RetryResult::Success(result);
                }
                Ok(Err(error)) => {
                    // Record failure to circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_failure();
                    }

                    // Check if we should retry
                    if !self.should_retry(&error) {
                        warn!(
                            operation_name = %operation_name,
                            error = %error.to_error_message(),
                            "Non-retryable error"
                        );
                        return RetryResult::Failed {
                            error,
                            attempts,
                            last_delay_ms,
                        };
                    }

                    if attempts >= self.config.max_retries {
                        warn!(
                            operation_name = %operation_name,
                            attempts = attempts,
                            error = %error.to_error_message(),
                            "Max retries exceeded"
                        );
                        return RetryResult::Failed {
                            error,
                            attempts,
                            last_delay_ms,
                        };
                    }

                    let delay = self.policy.config().calculate_delay(attempts);
                    last_delay_ms = Some(delay.as_millis() as u64);

                    info!(
                        operation_name = %operation_name,
                        attempt = attempts,
                        delay_ms = delay.as_millis(),
                        error = %error.to_error_message(),
                        "Retrying after delay"
                    );

                    // Wait with cancellation support
                    if self.wait_with_cancellation(delay).await {
                        return RetryResult::Cancelled;
                    }
                }
                Err(_) => {
                    // Record failure to circuit breaker
                    if let Some(cb) = &self.circuit_breaker {
                        cb.record_failure();
                    }

                    // Timeout - create error using trait method
                    if !self.config.retry_on_timeout || attempts >= self.config.max_retries {
                        warn!(
                            operation_name = %operation_name,
                            attempts = attempts,
                            "Operation timed out"
                        );
                        return RetryResult::Failed {
                            error: E::create_timeout_error(operation_name, self.config.attempt_timeout_ms),
                            attempts,
                            last_delay_ms,
                        };
                    }

                    let delay = self.policy.config().calculate_delay(attempts);
                    last_delay_ms = Some(delay.as_millis() as u64);

                    info!(
                        operation_name = %operation_name,
                        attempt = attempts,
                        delay_ms = delay.as_millis(),
                        "Retrying after timeout"
                    );

                    // Wait with cancellation support
                    if self.wait_with_cancellation(delay).await {
                        return RetryResult::Cancelled;
                    }
                }
            }
        }
    }
}

/// Type alias for backward compatibility
pub type AgentRetryExecutor = RetryExecutor<AgentError>;

/// Builder for creating retry executors
pub struct RetryExecutorBuilder<E: RetryableError = AgentError> {
    config: RetryConfig,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    cancellation_token: Option<CancellationToken>,
    _phantom: PhantomData<E>,
}

impl<E: RetryableError> RetryExecutorBuilder<E> {
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
            circuit_breaker: None,
            cancellation_token: None,
            _phantom: PhantomData,
        }
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    pub fn base_delay(mut self, delay_ms: u64) -> Self {
        self.config.base_delay_ms = delay_ms;
        self
    }

    pub fn max_delay(mut self, delay_ms: u64) -> Self {
        self.config.max_delay_ms = delay_ms;
        self
    }

    pub fn exponential_backoff(mut self) -> Self {
        self.config.backoff_strategy = BackoffStrategy::Exponential;
        self
    }

    pub fn exponential_with_jitter(mut self) -> Self {
        self.config.backoff_strategy = BackoffStrategy::ExponentialWithJitter;
        self
    }

    pub fn fixed_backoff(mut self) -> Self {
        self.config.backoff_strategy = BackoffStrategy::Fixed;
        self
    }

    pub fn linear_backoff(mut self) -> Self {
        self.config.backoff_strategy = BackoffStrategy::Linear;
        self
    }

    pub fn retry_on_timeout(mut self, retry: bool) -> Self {
        self.config.retry_on_timeout = retry;
        self
    }

    pub fn retry_on_transient(mut self, retry: bool) -> Self {
        self.config.retry_on_transient = retry;
        self
    }

    pub fn attempt_timeout(mut self, timeout_ms: u64) -> Self {
        self.config.attempt_timeout_ms = timeout_ms;
        self
    }

    /// Add circuit breaker for resilience
    pub fn with_circuit_breaker(mut self, cb: Arc<CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(cb);
        self
    }

    /// Add cancellation token for graceful shutdown
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    pub fn build(self) -> RetryExecutor<E> {
        RetryExecutor {
            policy: RetryPolicy::new(self.config.clone()),
            config: self.config,
            circuit_breaker: self.circuit_breaker,
            cancellation_token: self.cancellation_token,
            _phantom: PhantomData,
        }
    }
}

impl<E: RetryableError> Default for RetryExecutorBuilder<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for backward compatible builder
pub type AgentRetryExecutorBuilder = RetryExecutorBuilder<AgentError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_executor_success() {
        let executor = RetryExecutorBuilder::new()
            .max_retries(3)
            .build();

        let result = executor
            .execute("test_op", || async { Ok::<_, AgentError>(42) })
            .await;

        assert!(result.is_success());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_executor_retryable_error() {
        let executor = RetryExecutorBuilder::new()
            .max_retries(3)
            .fixed_backoff()
            .base_delay(10)
            .build();

        let attempts = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = executor
            .execute("test_op", move || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::Relaxed);
                    if count < 2 {
                        Err(AgentError::timeout("test", 100))
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert!(result.is_success());
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_retry_executor_max_retries_exceeded() {
        let executor = RetryExecutorBuilder::new()
            .max_retries(2)
            .fixed_backoff()
            .base_delay(10)
            .build();

        let result = executor
            .execute("test_op", || async {
                Err::<i32, _>(AgentError::timeout("test", 100))
            })
            .await;

        assert!(result.is_failed());
    }

    #[tokio::test]
    async fn test_retry_executor_cancellation() {
        let token = CancellationToken::new();
        let executor = RetryExecutorBuilder::new()
            .max_retries(10)
            .base_delay(100)
            .with_cancellation_token(token.clone())
            .build();

        // Cancel immediately
        token.cancel();

        let result = executor
            .execute("test_op", || async {
                Err::<i32, _>(AgentError::timeout("test", 100))
            })
            .await;

        assert!(result.is_cancelled());
    }

    #[tokio::test]
    async fn test_retry_executor_circuit_breaker() {
        let cb = Arc::new(CircuitBreaker::with_defaults());

        // Trip the circuit breaker
        for _ in 0..5 {
            cb.record_failure();
        }

        let executor = RetryExecutorBuilder::new()
            .max_retries(3)
            .with_circuit_breaker(cb)
            .build();

        let result = executor
            .execute("test_op", || async { Ok::<_, AgentError>(42) })
            .await;

        assert!(result.is_circuit_open());
    }

    #[tokio::test]
    async fn test_retry_result_map() {
        let result: RetryResult<i32, AgentError> = RetryResult::Success(42);
        let mapped = result.map(|x| x * 2);
        assert_eq!(mapped.unwrap(), 84);
    }
}
