use crate::error::Result;
use crate::llm::config::Provider;
use crate::llm::error::LLMResult;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

/// 消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: String },
    #[serde(rename = "assistant")]
    Assistant { content: String },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message::System { content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Message::User { content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Message::Assistant { content: content.into() }
    }
}

/// 补全请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stream: bool,
}

impl CompletionRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// 补全响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
    pub provider: Provider,
}

/// LLM 客户端 Trait
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// 返回提供商信息
    fn provider(&self) -> Provider;

    /// 返回当前使用的模型
    fn model(&self) -> Option<String>;

    /// 生成文本补全
    async fn complete(&self, request: CompletionRequest) -> LLMResult<CompletionResponse>;

    /// 流式生成
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> LLMResult<Box<dyn Stream<Item = Result<String>> + Send + Unpin>>;

    /// 获取 embeddings
    async fn embed(&self, input: &str) -> LLMResult<Vec<f32>>;

    /// 检查健康状态
    async fn health_check(&self) -> LLMResult<bool>;
}
