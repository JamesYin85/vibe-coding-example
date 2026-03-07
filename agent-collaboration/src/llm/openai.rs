use crate::error::Result;
use crate::llm::client::{CompletionRequest, CompletionResponse, LLMClient, Message, Usage};
use crate::llm::config::{LLMConfig, Provider};
use crate::llm::error::{LLMError, LLMResult};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, AUTHORIZATION};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{debug, instrument};

/// OpenAI 客户端
pub struct OpenAIClient {
    client: Client,
    config: LLMConfig,
}

impl OpenAIClient {
    pub fn new(config: LLMConfig) -> LLMResult<Self> {
        let api_key = config.openai_api_key.as_ref()
            .ok_or_else(|| LLMError::ConfigurationError {
                message: "OpenAI API key not configured".to_string(),
            })?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .map_err(|e| LLMError::ConfigurationError {
                    message: format!("Invalid API key format: {}", e),
                })?,
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
        self.config.openai_base_url.as_deref().unwrap_or("https://api.openai.com")
    }

    fn convert_request(&self, request: CompletionRequest) -> serde_json::Value {
        let model = request.model.unwrap_or_else(|| self.config.default_model.clone());

        let messages: Vec<serde_json::Value> = request.messages.iter().map(|msg| {
            match msg {
                Message::System { content } => serde_json::json!({
                    "role": "system",
                    "content": content
                }),
                Message::User { content } => serde_json::json!({
                    "role": "user",
                    "content": content
                }),
                Message::Assistant { content } => serde_json::json!({
                    "role": "assistant",
                    "content": content
                }),
            }
        }).collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if request.stream {
            body["stream"] = serde_json::json!(true);
        }

        body
    }
}

#[async_trait]
impl LLMClient for OpenAIClient {
    fn provider(&self) -> Provider {
        Provider::OpenAI
    }

    fn model(&self) -> Option<String> {
        Some(self.config.default_model.clone())
    }

    #[instrument(skip(self, request))]
    async fn complete(&self, request: CompletionRequest) -> LLMResult<CompletionResponse> {
        let url = format!("{}/v1/chat/completions", self.get_base_url());
        let body = self.convert_request(request);

        debug!(url = %url, "Sending request to OpenAI");

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

        let content = json["choices"][0]["message"]["content"]
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
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0),
        });

        Ok(CompletionResponse {
            content,
            model,
            usage,
            provider: Provider::OpenAI,
        })
    }

    #[instrument(skip(self, request))]
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> LLMResult<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/v1/chat/completions", self.get_base_url());
        let mut body = self.convert_request(request);
        body["stream"] = serde_json::json!(true);

        debug!(url = %url, "Starting stream request to OpenAI");

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

        // 返回一个简单的流包装器
        Ok(Box::new(OpenAIStream::new(response)))
    }

    #[instrument(skip(self))]
    async fn embed(&self, input: &str) -> LLMResult<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.get_base_url());

        let body = serde_json::json!({
            "model": "text-embedding-ada-002",
            "input": input,
        });

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

        let json: serde_json::Value = response.json().await.map_err(|e| {
            LLMError::InvalidResponse {
                details: format!("Failed to parse embedding response: {}", e),
            }
        })?;

        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| LLMError::InvalidResponse {
                details: "No embedding in response".to_string(),
            })?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(embedding)
    }

    #[instrument(skip(self))]
    async fn health_check(&self) -> LLMResult<bool> {
        let url = format!("{}/v1/models", self.get_base_url());

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|_e| LLMError::ProviderUnavailable {
                provider: "OpenAI".to_string(),
            })?;

        Ok(response.status().is_success())
    }
}

impl OpenAIClient {
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

/// OpenAI 流式响应包装器
pub struct OpenAIStream {
    response: reqwest::Response,
    buffer: String,
    done: bool,
}

impl OpenAIStream {
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            response,
            buffer: String::new(),
            done: false,
        }
    }
}

impl Stream for OpenAIStream {
    type Item = Result<String>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }

        // 简化实现：一次性读取所有内容
        // 实际生产环境需要更复杂的 SSE 解析
        Poll::Ready(None)
    }
}
