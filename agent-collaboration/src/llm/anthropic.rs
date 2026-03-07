use crate::error::Result;
use crate::llm::client::{CompletionRequest, CompletionResponse, LLMClient, Message, Usage};
use crate::llm::config::{LLMConfig, Provider};
use crate::llm::error::{LLMError, LLMResult};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{debug, instrument};

/// Anthropic 客户端
pub struct AnthropicClient {
    client: Client,
    config: LLMConfig,
}

impl AnthropicClient {
    pub fn new(config: LLMConfig) -> LLMResult<Self> {
        let api_key = config.anthropic_api_key.as_ref()
            .ok_or_else(|| LLMError::ConfigurationError {
                message: "Anthropic API key not configured".to_string(),
            })?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key)
                .map_err(|e| LLMError::ConfigurationError {
                    message: format!("Invalid API key format: {}", e),
                })?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static("2023-06-01"),
        );

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .default_headers(headers)
            .build()
            .map_err(|e| LLMError::InternalError {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self { client, config })
    }

    fn get_base_url(&self) -> &str {
        self.config.anthropic_base_url.as_deref().unwrap_or("https://api.anthropic.com")
    }

    fn convert_request(&self, request: CompletionRequest) -> serde_json::Value {
        let model = request.model.unwrap_or_else(|| self.config.default_model.clone());

        // 提取 system 消息
        let system_content: Option<String> = request.messages.iter()
            .find_map(|msg| match msg {
                Message::System { content } => Some(content.clone()),
                _ => None,
            });

        // 转换其他消息
        let messages: Vec<serde_json::Value> = request.messages.iter()
            .filter(|msg| !matches!(msg, Message::System { .. }))
            .map(|msg| {
                match msg {
                    Message::User { content } => serde_json::json!({
                        "role": "user",
                        "content": content
                    }),
                    Message::Assistant { content } => serde_json::json!({
                        "role": "assistant",
                        "content": content
                    }),
                    _ => serde_json::json!({}),
                }
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if let Some(system) = system_content {
            body["system"] = serde_json::json!(system);
        }

        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }

        body
    }
}

#[async_trait]
impl LLMClient for AnthropicClient {
    fn provider(&self) -> Provider {
        Provider::Anthropic
    }

    fn model(&self) -> Option<String> {
        Some(self.config.default_model.clone())
    }

    #[instrument(skip(self, request))]
    async fn complete(&self, request: CompletionRequest) -> LLMResult<CompletionResponse> {
        let url = format!("{}/v1/messages", self.get_base_url());
        let body = self.convert_request(request);

        debug!(url = %url, "Sending request to Anthropic");

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LLMError::Timeout { seconds: self.config.timeout_seconds }
                } else if e.is_connect() {
                    LLMError::NetworkError { message: e.to_string() }
                } else {
                    LLMError::ApiError {
                        message: e.to_string(),
                        code: None,
                    }
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Self::map_error(status, &error_text));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| {
            LLMError::InvalidResponse {
                details: format!("Failed to parse response: {}", e),
            }
        })?;

        // Anthropic 响应格式
        let content = json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| LLMError::InvalidResponse {
                details: "No content in response".to_string(),
            })?
            .to_string();

        let model = json["model"]
            .as_str()
            .unwrap_or(&self.config.default_model)
            .to_string();

        let usage = json.get("usage").map(|u| Usage {
            prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0),
            completion_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            total_tokens: u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0),
        });

        Ok(CompletionResponse {
            content,
            model,
            usage,
            provider: Provider::Anthropic,
        })
    }

    #[instrument(skip(self, request))]
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> LLMResult<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/v1/messages", self.get_base_url());
        let mut body = self.convert_request(request);
        body["stream"] = serde_json::json!(true);

        debug!(url = %url, "Starting stream request to Anthropic");

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::ApiError {
                message: e.to_string(),
                code: None,
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Self::map_error(status, &error_text));
        }

        Ok(Box::new(AnthropicStream::new(response)))
    }

    #[instrument(skip(self))]
    async fn embed(&self, _input: &str) -> LLMResult<Vec<f32>> {
        // Anthropic 目前不支持 embeddings API
        Err(LLMError::InvalidRequest {
            details: "Anthropic does not support embeddings API".to_string(),
        })
    }

    #[instrument(skip(self))]
    async fn health_check(&self) -> LLMResult<bool> {
        // Anthropic 没有专门的 health check 端点
        // 通过尝试创建一个简单的请求来验证
        let url = format!("{}/v1/messages", self.get_base_url());

        let body = serde_json::json!({
            "model": "claude-3-haiku-20240307",
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}],
        });

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|_| LLMError::ProviderUnavailable {
                provider: "Anthropic".to_string(),
            })?;

        // 即使返回错误（如 401），也说明服务可达
        Ok(response.status() != 503)
    }
}

impl AnthropicClient {
    fn map_error(status: reqwest::StatusCode, body: &str) -> LLMError {
        match status.as_u16() {
            401 => LLMError::AuthenticationFailed {
                reason: body.to_string(),
            },
            429 => LLMError::RateLimited { retry_after: None },
            500 | 502 | 503 | 504 => LLMError::ServiceUnavailable,
            400 => LLMError::InvalidRequest {
                details: body.to_string(),
            },
            _ => LLMError::ApiError {
                message: body.to_string(),
                code: Some(status.to_string()),
            },
        }
    }
}

/// Anthropic 流式响应包装器
pub struct AnthropicStream {
    response: reqwest::Response,
    buffer: String,
    done: bool,
}

impl AnthropicStream {
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            response,
            buffer: String::new(),
            done: false,
        }
    }
}

impl Stream for AnthropicStream {
    type Item = Result<String>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }

        // 简化实现
        Poll::Ready(None)
    }
}
