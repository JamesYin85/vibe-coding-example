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
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{AgentError, Result};
use crate::retry::policy::{BackoffStrategy, RetryConfig, RetryPolicy};
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn, instrument};

/// Result of a retry operation
#[derive(Debug)]
pub enum RetryResult<T> {
    /// Operation succeeded
    Success(T),
    /// Operation failed after all retries
    Failed {
        error: AgentError,
        attempts: u32,
        last_delay_ms: Option<u64>,
    },
    /// Operation was cancelled
    Cancelled,
}

impl<T> RetryResult<T> {
    pub fn is_success(&self) -> bool {
        matches!(self, RetryResult::Success(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, RetryResult::Failed { .. })
    }

    pub fn unwrap(self) -> T {
        match self {
            RetryResult::Success(value) => value,
            RetryResult::Failed { error, .. } => panic!("Called unwrap on a failed result: {}", error),
            RetryResult::Cancelled => panic!("Called unwrap on a cancelled result"),
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            RetryResult::Success(value) => value,
            _ => default,
        }
    }
}

/// Retry executor for operations that may fail
pub struct RetryExecutor {
    policy: RetryPolicy,
    config: RetryConfig,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            policy: RetryPolicy::new(config.clone()),
            config,
        }
    }

    pub fn with_policy(policy: RetryPolicy) -> Self {
        Self {
            policy,
            config: RetryConfig::default(),
        }
    }

    /// Execute an async operation with retry logic
    #[instrument(skip(self, operation), fields(operation_name))]
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> RetryResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let mut attempts = 0;
        let mut last_delay_ms = None;

        loop {
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
                    return RetryResult::Success(result);
                }
                Err(error) => {
                    // Check if we should retry
                    if !self.policy.should_retry(&error) {
                        warn!(
                            operation_name = %operation_name,
                            error = %error,
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
                            error = %error,
                            "Max retries exceeded"
                        );
                        return RetryResult::Failed {
                            error,
                            attempts,
                            last_delay_ms,
                        };
                    }

                    // Calculate delay and wait
                    let delay = self.policy.config().calculate_delay(attempts);
                    last_delay_ms = Some(delay.as_millis() as u64);

                    info!(
                        operation_name = %operation_name,
                        attempt = attempts,
                        delay_ms = delay.as_millis(),
                        error = %error,
                        "Retrying after delay"
                    );

                    sleep(delay).await;
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
    ) -> RetryResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if self.config.attempt_timeout_ms == 0 {
            return self.execute(operation_name, operation).await;
        }

        let timeout = Duration::from_millis(self.config.attempt_timeout_ms);

        let mut attempts = 0;
        let mut last_delay_ms = None;

        loop {
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
                    return RetryResult::Success(result);
                }
                Ok(Err(error)) => {
                    // Check if we should retry
                    if !self.policy.should_retry(&error) {
                        warn!(
                            operation_name = %operation_name,
                            error = %error,
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
                            error = %error,
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
                        error = %error,
                        "Retrying after delay"
                    );

                    sleep(delay).await;
                }
                Err(_) => {
                    // Timeout
                    let error = AgentError::timeout(
                        operation_name,
                        self.config.attempt_timeout_ms,
                    );

                    if !self.config.retry_on_timeout || attempts >= self.config.max_retries {
                        warn!(
                            operation_name = %operation_name,
                            attempts = attempts,
                            "Operation timed out"
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
                        "Retrying after timeout"
                    );

                    sleep(delay).await;
                }
            }
        }
    }
}

/// Builder for creating retry executors
pub struct RetryExecutorBuilder {
    config: RetryConfig,
}

impl RetryExecutorBuilder {
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
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

    pub fn build(self) -> RetryExecutor {
        RetryExecutor::new(self.config)
    }
}

impl Default for RetryExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

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
}
