//! LLM 接口层模块
//!
//! 提供统一的 LLM 调用接口，支持多个提供商和降级机制。

pub mod anthropic;
pub mod client;
pub mod config;
pub mod error;
pub mod fallback;
pub mod openai;

// 重新导出常用类型
pub use anthropic::AnthropicClient;
pub use client::{
    CompletionRequest, CompletionResponse, LLMClient, Message, Usage,
};
pub use config::{LLMConfig, Provider};
pub use error::{LLMError, LLMResult};
pub use fallback::{FallbackClient, FallbackClientBuilder, FallbackConfig};
pub use openai::OpenAIClient;
