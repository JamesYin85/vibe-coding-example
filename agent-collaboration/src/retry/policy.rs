//! Retry Policy Configuration
//!
//! Provides configurable retry policies with various backoff strategies.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info};

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    Fixed,
    /// Linear increasing delay: delay * attempt
    Linear,
    /// Exponential backoff: delay * 2^attempt
    Exponential,
    /// Exponential backoff with jitter to avoid thundering herd
    ExponentialWithJitter,
    /// Custom delays for each attempt
    Custom,
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self::ExponentialWithJitter
    }
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay cap in milliseconds
    pub max_delay_ms: u64,
    /// Backoff strategy
    pub backoff_strategy: BackoffStrategy,
    /// Custom delays (used when strategy is Custom)
    pub custom_delays_ms: Vec<u64>,
    /// Whether to retry on timeout errors
    pub retry_on_timeout: bool,
    /// Whether to retry on transient errors
    pub retry_on_transient: bool,
    /// Timeout for each individual attempt (0 = no timeout)
    pub attempt_timeout_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 30_000, // 30 seconds max
            backoff_strategy: BackoffStrategy::ExponentialWithJitter,
            custom_delays_ms: Vec::new(),
            retry_on_timeout: true,
            retry_on_transient: true,
            attempt_timeout_ms: 0,
        }
    }
}

impl RetryConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_base_delay(mut self, delay_ms: u64) -> Self {
        self.base_delay_ms = delay_ms;
        self
    }

    pub fn with_max_delay(mut self, delay_ms: u64) -> Self {
        self.max_delay_ms = delay_ms;
        self
    }

    pub fn with_backoff(mut self, strategy: BackoffStrategy) -> Self {
        self.backoff_strategy = strategy;
        self
    }

    pub fn with_retry_on_timeout(mut self, retry: bool) -> Self {
        self.retry_on_timeout = retry;
        self
    }

    pub fn with_attempt_timeout(mut self, timeout_ms: u64) -> Self {
        self.attempt_timeout_ms = timeout_ms;
        self
    }

    /// Calculate delay for a given attempt number (0-indexed)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let delay_ms = match self.backoff_strategy {
            BackoffStrategy::Fixed => self.base_delay_ms,
            BackoffStrategy::Linear => {
                self.base_delay_ms.saturating_mul(attempt as u64)
            }
            BackoffStrategy::Exponential => {
                let multiplier = 2u64.saturating_pow(attempt);
                self.base_delay_ms.saturating_mul(multiplier)
            }
            BackoffStrategy::ExponentialWithJitter => {
                let multiplier = 2u64.saturating_pow(attempt);
                let base = self.base_delay_ms.saturating_mul(multiplier);
                // Add jitter: ±20% random variation
                let jitter = (base as f64 * 0.2) as u64;
                let rand_offset = (attempt as u64 * 17) % (jitter.max(1) * 2);
                if rand_offset > jitter {
                    base.saturating_add(rand_offset - jitter)
                } else {
                    base.saturating_sub(jitter - rand_offset)
                }
            }
            BackoffStrategy::Custom => {
                self.custom_delays_ms
                    .get(attempt as usize)
                    .copied()
                    .unwrap_or(self.base_delay_ms)
            }
        };

        let capped_delay = delay_ms.min(self.max_delay_ms);
        Duration::from_millis(capped_delay)
    }

    /// Check if we should retry based on the error type
    pub fn should_retry(&self, error: &crate::error::AgentError) -> bool {
        if error.should_retry() {
            return true;
        }

        if self.retry_on_timeout && matches!(error, crate::error::AgentError::Timeout { .. }) {
            return true;
        }

        if self.retry_on_transient && error.is_recoverable() {
            return true;
        }

        false
    }
}

/// Retry policy that tracks state across retries
#[derive(Debug)]
pub struct RetryPolicy {
    config: RetryConfig,
    current_attempt: u32,
    total_retries: u32,
    last_error: Option<String>,
}

impl RetryPolicy {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            current_attempt: 0,
            total_retries: 0,
            last_error: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Check if we should attempt another retry
    pub fn should_retry(&self, error: &crate::error::AgentError) -> bool {
        if self.current_attempt >= self.config.max_retries {
            debug!(
                attempt = self.current_attempt,
                max_retries = self.config.max_retries,
                "Max retries exceeded"
            );
            return false;
        }

        self.config.should_retry(error)
    }

    /// Record an attempt and return the delay before next attempt
    pub fn record_attempt(&mut self, error: &crate::error::AgentError) -> Option<Duration> {
        if !self.should_retry(error) {
            return None;
        }

        self.current_attempt += 1;
        self.total_retries += 1;
        self.last_error = Some(error.to_string());

        let delay = self.config.calculate_delay(self.current_attempt);

        info!(
            attempt = self.current_attempt,
            max_retries = self.config.max_retries,
            delay_ms = delay.as_millis(),
            error = %error,
            "Scheduling retry"
        );

        Some(delay)
    }

    /// Reset the policy for a new operation
    pub fn reset(&mut self) {
        self.current_attempt = 0;
        self.last_error = None;
    }

    /// Get current attempt number
    pub fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    /// Get total retries across all operations
    pub fn total_retries(&self) -> u32 {
        self.total_retries
    }

    /// Get the last error message
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Get the config
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_backoff() {
        let config = RetryConfig::new()
            .with_backoff(BackoffStrategy::Fixed)
            .with_base_delay(100);

        assert_eq!(config.calculate_delay(1), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(100));
    }

    #[test]
    fn test_linear_backoff() {
        let config = RetryConfig::new()
            .with_backoff(BackoffStrategy::Linear)
            .with_base_delay(100);

        assert_eq!(config.calculate_delay(1), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(300));
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig::new()
            .with_backoff(BackoffStrategy::Exponential)
            .with_base_delay(100);

        assert_eq!(config.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig::new()
            .with_backoff(BackoffStrategy::Exponential)
            .with_base_delay(1000)
            .with_max_delay(5000);

        assert_eq!(config.calculate_delay(1), Duration::from_millis(2000));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(4000));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(5000)); // capped
        assert_eq!(config.calculate_delay(4), Duration::from_millis(5000)); // capped
    }

    #[test]
    fn test_retry_policy() {
        let mut policy = RetryPolicy::with_defaults();

        let error = crate::error::AgentError::timeout("test", 1000);

        assert!(policy.should_retry(&error));
        assert_eq!(policy.current_attempt(), 0);

        let delay = policy.record_attempt(&error);
        assert!(delay.is_some());
        assert_eq!(policy.current_attempt(), 1);
    }

    #[test]
    fn test_max_retries_exceeded() {
        let config = RetryConfig::new().with_max_retries(2);
        let mut policy = RetryPolicy::new(config);

        let error = crate::error::AgentError::timeout("test", 1000);

        // First retry
        assert!(policy.record_attempt(&error).is_some());
        // Second retry
        assert!(policy.record_attempt(&error).is_some());
        // Should not allow more
        assert!(!policy.should_retry(&error));
        assert!(policy.record_attempt(&error).is_none());
    }
}
