//! Circuit Breaker Pattern
//!
//! Provides resilience by preventing cascading failures in distributed systems.
//!
//! # States
//!
//! - **Closed**: Normal operation, requests flow through, failures are counted
//! - **Open**: Requests are blocked, waiting for reset timeout
//! - **HalfOpen**: Limited requests allowed to test if the service has recovered
//!
//! # Example
//!
//! ```rust,no_run
//! use agent_collaboration::retry::{CircuitBreaker, CircuitBreakerConfig};
//!
//! let cb = CircuitBreaker::new(CircuitBreakerConfig {
//!     failure_threshold: 5,      // Open after 5 failures
//!     failure_window_secs: 60,   // Within 60 seconds
//!     reset_timeout_secs: 30,    // Try half-open after 30s
//!     half_open_max_calls: 3,    // Allow 3 test calls
//!     half_open_timeout_secs: 10,
//! });
//!
//! if cb.is_call_allowed() {
//!     // Make your request
//!     cb.record_success(); // or cb.record_failure()
//! }
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tracing::{debug, info, warn};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, normal operation
    Closed = 0,
    /// Circuit is open, requests are blocked
    Open = 2,
    /// Circuit is half-open, limited requests allowed
    HalfOpen = 1,
}

impl Default for CircuitState {
    fn default() -> Self {
        Self::Closed
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Time window for counting failures (seconds)
    pub failure_window_secs: u64,
    /// Time to wait before attempting to close circuit after failures (seconds)
    pub reset_timeout_secs: u64,
    /// Half-open state: allow limited requests through
    pub half_open_max_calls: u32,
    /// Time to wait in half-open state before trying to close (seconds)
    pub half_open_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window_secs: 60,
            reset_timeout_secs: 30,
            half_open_max_calls: 3,
            half_open_timeout_secs: 10,
        }
    }
}

/// Circuit Breaker implementation
pub struct CircuitBreaker {
    state: AtomicU32,
    failure_count: AtomicU32,
    last_failure_time: AtomicU64,
    half_open_calls: AtomicU32,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU32::new(CircuitState::Closed as u32),
            failure_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            half_open_calls: AtomicU32::new(0),
            config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Check if requests are allowed
    pub fn is_call_allowed(&self) -> bool {
        let state = self.get_state();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                let now = Self::current_time_millis();
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);
                if last_failure > 0 {
                    let elapsed = now - last_failure;
                    if elapsed > self.config.reset_timeout_secs * 1000 {
                        self.transition_to_half_open();
                    }
                }
                false // Block calls when open
            }
            CircuitState::HalfOpen => {
                // Allow limited calls in half-open state
                let calls = self.half_open_calls.fetch_add(1, Ordering::Relaxed);
                calls < self.config.half_open_max_calls
            }
        }
    }

    /// Record a successful call
    pub fn record_success(&self) {
        let state = self.get_state();
        if state == CircuitState::HalfOpen {
            let calls = self.half_open_calls.fetch_add(1, Ordering::Relaxed);
            if calls >= self.config.half_open_max_calls {
                // Transition back to closed after enough successful calls
                self.transition_to_closed();
                info!("Circuit breaker recovered to CLOSED state");
            }
        } else if state == CircuitState::Open {
            // Reset failure count on success
            self.failure_count.store(0, Ordering::Relaxed);
        }
    }

    /// Record a failed call
    pub fn record_failure(&self) {
        let now = Self::current_time_millis();
        let last_failure = self.last_failure_time.load(Ordering::Relaxed);
        let mut failure_count = self.failure_count.load(Ordering::Relaxed);

        // Check if we're within the failure window
        if last_failure > 0 && (now - last_failure) > self.config.failure_window_secs * 1000 {
            // Reset failure count if outside window
            failure_count = 0;
        }

        failure_count += 1;
        self.failure_count.store(failure_count, Ordering::Relaxed);
        self.last_failure_time.store(now, Ordering::Relaxed);

        let state = self.get_state();
        if state == CircuitState::Closed && failure_count >= self.config.failure_threshold {
            self.transition_to_open();
            warn!(
                "Circuit breaker tripped to OPEN state after {} failures",
                failure_count
            );
        } else if state == CircuitState::HalfOpen {
            // Transition back to open if too many failures in half-open
            if failure_count >= self.config.failure_threshold {
                self.transition_to_open();
                warn!(
                    "Circuit breaker returned to OPEN state after {} failures in half-open",
                    failure_count
                );
            }
        }
    }

    fn get_state(&self) -> CircuitState {
        match self.state.load(Ordering::Relaxed) {
            0 => CircuitState::Closed,
            1 => CircuitState::HalfOpen,
            2 => CircuitState::Open,
            _ => CircuitState::Closed,
        }
    }

    fn transition_to_half_open(&self) {
        self.state.store(CircuitState::HalfOpen as u32, Ordering::Relaxed);
        self.half_open_calls.store(0, Ordering::Relaxed);
        debug!("Circuit breaker transitioned to HALF_OPEN state");
    }

    fn transition_to_closed(&self) {
        self.state.store(CircuitState::Closed as u32, Ordering::Relaxed);
        self.failure_count.store(0, Ordering::Relaxed);
        debug!("Circuit breaker transitioned to CLOSED state");
    }

    fn transition_to_open(&self) {
        self.state.store(CircuitState::Open as u32, Ordering::Relaxed);
        debug!("Circuit breaker transitioned to OPEN state");
    }

    fn current_time_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Get current failure count
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Reset the circuit breaker
    pub fn reset(&self) {
        self.state.store(CircuitState::Closed as u32, Ordering::Relaxed);
        self.failure_count.store(0, Ordering::Relaxed);
        self.last_failure_time.store(0, Ordering::Relaxed);
        self.half_open_calls.store(0, Ordering::Relaxed);
        info!("Circuit breaker reset to CLOSED state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::with_defaults();
        assert!(cb.is_call_allowed());
    }

    #[test]
    fn test_circuit_breaker_transitions() {
        let cb = CircuitBreaker::with_defaults();

        // Should start in closed state
        assert!(cb.is_call_allowed());

        // Record failures until threshold
        for _ in 0..5 {
            cb.record_failure();
        }

        // Should now be in open state
        assert!(!cb.is_call_allowed());
    }

    #[test]
    fn test_circuit_breaker_recovery() {
        let cb = CircuitBreaker::with_defaults();

        // Trigger failures
        for _ in 0..5 {
            cb.record_failure();
        }

        // Reset to simulate recovery
        cb.reset();

        // Should be back to closed state
        assert!(cb.is_call_allowed());
    }
}
