//! Retryable Error Trait
//!
//! Defines a common interface for error types that can be retried.

use std::time::Duration;

/// Trait for errors that can determine if they should be retried.
///
/// This trait allows the retry mechanism to work with any error type
/// that can indicate whether it's retryable, a timeout, or transient.
pub trait RetryableError: std::fmt::Debug + Send + Sync + 'static {
    /// Whether this error should trigger a retry attempt.
    fn is_retryable(&self) -> bool;

    /// Whether this error represents a timeout.
    fn is_timeout(&self) -> bool;

    /// Whether this error is transient/recoverable.
    ///
    /// Transient errors are typically caused by temporary conditions
    /// like network issues or service overload.
    fn is_transient(&self) -> bool;

    /// Optional: Suggested delay before retrying.
    ///
    /// Returns `None` if no specific delay is suggested.
    fn retry_after(&self) -> Option<Duration> {
        None
    }

    /// Convert to a user-friendly error message.
    fn to_error_message(&self) -> String {
        format!("{:?}", self)
    }

    /// Create a timeout error for this error type.
    ///
    /// This is used by the retry executor when an operation times out.
    fn create_timeout_error(operation: &str, duration_ms: u64) -> Self;
}
