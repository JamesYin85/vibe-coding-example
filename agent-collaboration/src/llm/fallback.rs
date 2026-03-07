use crate::error::Result;
use crate::llm::client::{CompletionRequest, CompletionResponse, LLMClient};
use crate::llm::config::{LLMConfig, Provider};
use crate::llm::error::{LLMError, LLMResult};
use crate::llm::openai::OpenAIClient;
use crate::llm::anthropic::AnthropicClient;
use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// 降级配置
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub exponential_backoff: bool,
    pub fallback_on_rate_limit: bool,
    pub fallback_on_content_filter: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delay_ms: 1000,
            exponential_backoff: true,
            fallback_on_rate_limit: true,
            fallback_on_content_filter: true,
        }
    }
}

/// 降级客户端，支持多种 LLM 提供商的降级策略
pub struct FallbackClient {
    primary: Box<dyn LLMClient>,
    secondary: Option<Box<dyn LLMClient>>,
    config: FallbackConfig,
    provider_health: Arc<RwLock<HashMap<Provider, bool>>>,
}

impl FallbackClient {
    pub fn new(config: LLMConfig) -> LLMResult<Self> {
        let primary = Self::create_client(&config.default_provider, config.clone())?;

        let secondary = if let Some(fallback_provider) = &config.fallback_provider {
            Some(Self::create_client(fallback_provider, config.clone())?)
        } else {
            None
        };

        Ok(Self {
            primary,
            secondary,
            config: FallbackConfig::default(),
            provider_health: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn create_client(provider: &Provider, config: LLMConfig) -> LLMResult<Box<dyn LLMClient>> {
        match provider {
            Provider::OpenAI => Ok(Box::new(OpenAIClient::new(config)?)),
            Provider::Anthropic => Ok(Box::new(AnthropicClient::new(config)?)),
        }
    }

    pub fn builder(config: LLMConfig) -> FallbackClientBuilder {
        FallbackClientBuilder::new(config)
    }

    pub fn with_config(mut self, config: FallbackConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_primary(mut self, client: Box<dyn LLMClient>) -> Self {
        self.primary = client;
        self
    }

    pub fn with_secondary(mut self, client: Box<dyn LLMClient>) -> Self {
        self.secondary = Some(client);
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    pub fn with_retry_delay(mut self, retry_delay_ms: u64) -> Self {
        self.config.retry_delay_ms = retry_delay_ms;
        self
    }

    pub fn with_exponential_backoff(mut self, exponential_backoff: bool) -> Self {
        self.config.exponential_backoff = exponential_backoff;
        self
    }

    pub fn with_fallback_on_rate_limit(mut self, fallback_on_rate_limit: bool) -> Self {
        self.config.fallback_on_rate_limit = fallback_on_rate_limit;
        self
    }

    pub fn with_fallback_on_content_filter(mut self, fallback_on_content_filter: bool) -> Self {
        self.config.fallback_on_content_filter = fallback_on_content_filter;
        self
    }

    /// 计算重试延迟（指数退避）
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.config.retry_delay_ms;
        if self.config.exponential_backoff {
            let multiplier = 2u64.saturating_pow(attempt);
            Duration::from_millis(base_delay.saturating_mul(multiplier))
        } else {
            Duration::from_millis(base_delay)
        }
    }

    /// 检查是否应该降级到备用提供商
    fn should_fallback(&self, error: &LLMError) -> bool {
        match error {
            LLMError::RateLimited { .. } => self.config.fallback_on_rate_limit,
            LLMError::ContentFiltered { .. } => self.config.fallback_on_content_filter,
            LLMError::ProviderUnavailable { .. } => true,
            LLMError::ModelOverloaded { .. } => true,
            LLMError::ServiceUnavailable => true,
            _ => false,
        }
    }

    /// 带降级和重试的补全请求
    #[instrument(skip(self, request))]
    pub async fn complete(&self, request: CompletionRequest) -> LLMResult<CompletionResponse> {
        let mut last_error: Option<LLMError> = None;

        // 尝试主提供商
        for attempt in 0..self.config.max_retries {
            match self.primary.complete(request.clone()).await {
                Ok(response) => {
                    info!(
                        provider = ?self.primary.provider(),
                        attempt = attempt + 1,
                        "Request completed successfully"
                    );
                    self.mark_provider_healthy(self.primary.provider()).await;
                    return Ok(response);
                }
                Err(e) => {
                    warn!(
                        provider = ?self.primary.provider(),
                        attempt = attempt + 1,
                        error = %e,
                        "Request failed"
                    );

                    last_error = Some(e.clone());

                    // 如果错误可重试，等待后重试
                    if e.is_retryable() && attempt + 1 < self.config.max_retries {
                        let delay = self.calculate_delay(attempt);
                        debug!(delay_ms = delay.as_millis(), "Waiting before retry");
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    // 如果应该降级，跳出循环尝试备用
                    if self.should_fallback(&e) {
                        break;
                    }

                    // 否则直接返回错误
                    return Err(e);
                }
            }
        }

        // 尝试备用提供商
        if let Some(secondary) = &self.secondary {
            info!(
                primary_provider = ?self.primary.provider(),
                secondary_provider = ?secondary.provider(),
                "Falling back to secondary provider"
            );

            match secondary.complete(request).await {
                Ok(response) => {
                    self.mark_provider_healthy(secondary.provider()).await;
                    return Ok(response);
                }
                Err(e) => {
                    error!(
                        provider = ?secondary.provider(),
                        error = %e,
                        "Secondary provider also failed"
                    );
                    last_error = Some(e);
                }
            }
        }

        // 所有尝试都失败了
        Err(last_error.unwrap_or(LLMError::ServiceUnavailable))
    }

    async fn mark_provider_healthy(&self, provider: Provider) {
        let mut health = self.provider_health.write().await;
        health.insert(provider, true);
    }

    async fn mark_provider_unhealthy(&self, provider: Provider) {
        let mut health = self.provider_health.write().await;
        health.insert(provider, false);
    }

    pub async fn get_provider_health(&self, provider: Provider) -> bool {
        let health = self.provider_health.read().await;
        health.get(&provider).copied().unwrap_or(true)
    }
}

/// 降级客户端构建器
pub struct FallbackClientBuilder {
    config: LLMConfig,
    fallback_config: FallbackConfig,
    primary: Option<Box<dyn LLMClient>>,
    secondary: Option<Box<dyn LLMClient>>,
}

impl FallbackClientBuilder {
    pub fn new(config: LLMConfig) -> Self {
        Self {
            config,
            fallback_config: FallbackConfig::default(),
            primary: None,
            secondary: None,
        }
    }

    pub fn fallback_config(mut self, config: FallbackConfig) -> Self {
        self.fallback_config = config;
        self
    }

    pub fn primary(mut self, client: Box<dyn LLMClient>) -> Self {
        self.primary = Some(client);
        self
    }

    pub fn secondary(mut self, client: Box<dyn LLMClient>) -> Self {
        self.secondary = Some(client);
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.fallback_config.max_retries = max_retries;
        self
    }

    pub fn retry_delay(mut self, delay_ms: u64) -> Self {
        self.fallback_config.retry_delay_ms = delay_ms;
        self
    }

    pub fn exponential_backoff(mut self, enabled: bool) -> Self {
        self.fallback_config.exponential_backoff = enabled;
        self
    }

    pub fn build(self) -> LLMResult<FallbackClient> {
        let primary = self.primary.unwrap_or_else(|| {
            FallbackClient::create_client(&self.config.default_provider, self.config.clone())
                .expect("Failed to create primary client")
        });

        let secondary = match self.secondary {
            Some(s) => Some(s),
            None => {
                if let Some(fallback_provider) = &self.config.fallback_provider {
                    Some(FallbackClient::create_client(fallback_provider, self.config.clone())?)
                } else {
                    None
                }
            }
        };

        Ok(FallbackClient {
            primary,
            secondary,
            config: self.fallback_config,
            provider_health: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl LLMClient for FallbackClient {
    fn provider(&self) -> Provider {
        self.primary.provider()
    }

    fn model(&self) -> Option<String> {
        self.primary.model()
    }

    async fn complete(&self, request: CompletionRequest) -> LLMResult<CompletionResponse> {
        self.complete(request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> LLMResult<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
        // 流式请求先尝试主提供商
        self.primary.complete_stream(request).await
    }

    async fn embed(&self, input: &str) -> LLMResult<Vec<f32>> {
        self.primary.embed(input).await
    }

    async fn health_check(&self) -> LLMResult<bool> {
        let primary_healthy = self.primary.health_check().await.unwrap_or(false);

        let secondary_healthy = if let Some(secondary) = &self.secondary {
            secondary.health_check().await.unwrap_or(false)
        } else {
            true
        };

        Ok(primary_healthy || secondary_healthy)
    }
}
