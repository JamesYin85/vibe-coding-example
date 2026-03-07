use thiserror::Error;
use std::time::Duration;

#[derive(Debug, Clone, Error)]
pub enum LLMError {
    #[error("API error: {message}")]
    ApiError {
        message: String,
        code: Option<String>,
    },

    #[error("Rate limit exceeded, retry after {retry_after:?} seconds")]
    RateLimited {
        retry_after: Option<Duration>,
    },

    #[error("Authentication failed: {reason}")]
    AuthenticationFailed {
        reason: String,
    },

    #[error("Invalid request: {details}")]
    InvalidRequest {
        details: String,
    },

    #[error("Invalid response: {details}")]
    InvalidResponse {
        details: String,
    },

    #[error("Request timeout after {seconds}s")]
    Timeout {
        seconds: u64,
    },

    #[error("Provider unavailable: {provider}")]
    ProviderUnavailable {
        provider: String,
    },

    #[error("Content filtered: {reason}")]
    ContentFiltered {
        reason: String,
    },

    #[error("Context length exceeded: {actual} > {max}")]
    ContextTooLong {
        actual: usize,
        max: usize,
    },

    #[error("Model overloaded: {model}")]
    ModelOverloaded {
        model: String,
    },

    #[error("Service temporarily unavailable")]
    ServiceUnavailable,

    #[error("Network error: {message}")]
    NetworkError {
        message: String,
    },

    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
    },

    #[error("Internal error: {message}")]
    InternalError {
        message: String,
    },
}

impl LLMError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LLMError::RateLimited { .. }
            | LLMError::Timeout { .. }
            | LLMError::ServiceUnavailable
            | LLMError::NetworkError { .. }
            | LLMError::ModelOverloaded { .. }
        )
    }

    pub fn user_message(&self) -> String {
        match self {
            LLMError::ApiError { message, .. } => message.clone(),
            LLMError::AuthenticationFailed { reason } => reason.clone(),
            LLMError::InvalidRequest { details } => details.clone(),
            LLMError::InvalidResponse { details } => details.clone(),
            LLMError::ContentFiltered { reason } => reason.clone(),
            LLMError::ContextTooLong { actual, max } => {
                format!("Context too long: {} > {}", actual, max)
            }
            LLMError::ConfigurationError { message } => message.clone(),
            LLMError::InternalError { message } => message.clone(),
            _ => "An error occurred".to_string(),
        }
    }

    pub fn should_use_fallback(&self) -> bool {
        matches!(
            self,
            LLMError::RateLimited { .. }
            | LLMError::ProviderUnavailable { .. }
            | LLMError::ModelOverloaded { .. }
        )
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            LLMError::RateLimited { retry_after } => *retry_after,
            LLMError::Timeout { seconds } => Some(Duration::from_secs(*seconds)),
            _ => None,
        }
    }
}

pub type LLMResult<T> = std::result::Result<T, LLMError>;
