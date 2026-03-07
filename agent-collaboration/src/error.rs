use std::error::Error;
use std::fmt;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Transient,
    Permanent,
    Configuration,
    Validation,
    Timeout,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Transient => write!(f, "Transient"),
            ErrorCategory::Permanent => write!(f, "Permanent"),
            ErrorCategory::Configuration => write!(f, "Configuration"),
            ErrorCategory::Validation => write!(f, "Validation"),
            ErrorCategory::Timeout => write!(f, "Timeout"),
        }
    }
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("[{agent_id}] Task understanding failed: {message}")]
    UnderstandingFailed {
        agent_id: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("[{agent_id}] Task decomposition failed: {message}")]
    DecompositionFailed {
        agent_id: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("[{agent_id}] Task execution failed: {message}")]
    ExecutionFailed {
        agent_id: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("[{agent_id}] Communication error: {message}")]
    CommunicationError {
        agent_id: String,
        message: String,
    },

    #[error("Invalid state transition: from '{from}' to '{to}'")]
    InvalidStateTransition {
        from: String,
        to: String,
    },

    #[error("Capability '{name}' not found")]
    CapabilityNotFound {
        name: String,
    },

    #[error("Capability '{name}' execution failed: {message}")]
    CapabilityExecutionFailed {
        name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Channel error: {message}")]
    ChannelError {
        message: String,
    },

    #[error("Task '{task_id}' was cancelled")]
    Cancelled {
        task_id: String,
    },

    #[error("Operation timed out after {duration_ms}ms: {operation}")]
    Timeout {
        operation: String,
        duration_ms: u64,
    },

    #[error("Agent '{agent_id}' not found")]
    AgentNotFound {
        agent_id: String,
    },

    #[error("Invalid input: {message}")]
    InvalidInput {
        message: String,
    },

    #[error("{message}. Suggestion: {suggestion}")]
    Recoverable {
        message: String,
        suggestion: String,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
    },
}

impl AgentError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            AgentError::Timeout { .. } => ErrorCategory::Timeout,
            AgentError::Cancelled { .. } => ErrorCategory::Transient,
            AgentError::ChannelError { .. } => ErrorCategory::Transient,
            AgentError::CommunicationError { .. } => ErrorCategory::Transient,
            AgentError::CapabilityNotFound { .. } => ErrorCategory::Configuration,
            AgentError::InvalidStateTransition { .. } => ErrorCategory::Validation,
            AgentError::InvalidInput { .. } => ErrorCategory::Validation,
            AgentError::AgentNotFound { .. } => ErrorCategory::Configuration,
            AgentError::Recoverable { .. } => ErrorCategory::Transient,
            AgentError::UnderstandingFailed { .. }
            | AgentError::DecompositionFailed { .. }
            | AgentError::ExecutionFailed { .. }
            | AgentError::CapabilityExecutionFailed { .. } => ErrorCategory::Permanent,
            AgentError::Internal { .. } => ErrorCategory::Permanent,
        }
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(
            self.category(),
            ErrorCategory::Transient | ErrorCategory::Timeout
        )
    }

    pub fn should_retry(&self) -> bool {
        matches!(
            self,
            AgentError::Timeout { .. }
            | AgentError::ChannelError { .. }
            | AgentError::CommunicationError { .. }
        )
    }

    pub fn understanding_failed(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::UnderstandingFailed {
            agent_id: agent_id.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn understanding_failed_with_source(
        agent_id: impl Into<String>,
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::UnderstandingFailed {
            agent_id: agent_id.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn decomposition_failed(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::DecompositionFailed {
            agent_id: agent_id.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn execution_failed(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            agent_id: agent_id.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn execution_failed_with_source(
        agent_id: impl Into<String>,
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::ExecutionFailed {
            agent_id: agent_id.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn communication_error(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CommunicationError {
            agent_id: agent_id.into(),
            message: message.into(),
        }
    }

    pub fn capability_not_found(name: impl Into<String>) -> Self {
        Self::CapabilityNotFound {
            name: name.into(),
        }
    }

    pub fn capability_failed(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CapabilityExecutionFailed {
            name: name.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn channel_error(message: impl Into<String>) -> Self {
        Self::ChannelError {
            message: message.into(),
        }
    }

    pub fn timeout(operation: impl Into<String>, duration_ms: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration_ms,
        }
    }

    pub fn cancelled(task_id: impl Into<String>) -> Self {
        Self::Cancelled {
            task_id: task_id.into(),
        }
    }

    pub fn recoverable(message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::Recoverable {
            message: message.into(),
            suggestion: suggestion.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn log(&self) {
        match self.category() {
            ErrorCategory::Transient | ErrorCategory::Timeout => {
                warn!(error = %self, category = %self.category(), "Recoverable error occurred");
            }
            ErrorCategory::Permanent => {
                warn!(error = %self, category = %self.category(), "Permanent error occurred");
            }
            ErrorCategory::Configuration | ErrorCategory::Validation => {
                warn!(error = %self, category = %self.category(), "Configuration/Validation error");
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, AgentError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category() {
        let err = AgentError::timeout("test", 1000);
        assert_eq!(err.category(), ErrorCategory::Timeout);
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_error_constructors() {
        let err = AgentError::understanding_failed("agent-1", "Failed to parse");
        assert!(err.to_string().contains("agent-1"));
        assert!(err.to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_recoverable_error() {
        let err = AgentError::recoverable("Connection lost", "Retry in 5 seconds");
        assert!(err.is_recoverable());
        assert!(err.to_string().contains("Retry in 5 seconds"));
    }

    #[test]
    fn test_should_retry() {
        let err = AgentError::timeout("test", 1000);
        assert!(err.should_retry());

        let err = AgentError::capability_not_found("test");
        assert!(!err.should_retry());
    }
}
