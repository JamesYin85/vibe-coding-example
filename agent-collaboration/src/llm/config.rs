use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    OpenAI,
    Anthropic,
}

impl Default for Provider {
    fn default() -> Self {
        Self::OpenAI
    }
}

#[derive(Debug, Clone)]
pub struct LLMConfig {
    pub default_provider: Provider,
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub default_model: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub fallback_provider: Option<Provider>,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            default_provider: Provider::OpenAI,
            openai_api_key: None,
            openai_base_url: None,
            anthropic_api_key: None,
            anthropic_base_url: None,
            default_model: "gpt-4o".to_string(),
            timeout_seconds: 30,
            max_retries: 3,
            fallback_provider: None,
        }
    }
}

impl LLMConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_provider(mut self, provider: Provider) -> Self {
        self.default_provider = provider;
        self
    }

    pub fn with_openai_key(mut self, key: impl Into<String>) -> Self {
        self.openai_api_key = Some(key.into());
        self
    }

    pub fn with_openai_base_url(mut self, url: impl Into<String>) -> Self {
        self.openai_base_url = Some(url.into());
        self
    }

    pub fn with_anthropic_key(mut self, key: impl Into<String>) -> Self {
        self.anthropic_api_key = Some(key.into());
        self
    }

    pub fn with_anthropic_base_url(mut self, url: impl Into<String>) -> Self {
        self.anthropic_base_url = Some(url.into());
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn with_fallback_provider(mut self, provider: Provider) -> Self {
        self.fallback_provider = Some(provider);
        self
    }
}

