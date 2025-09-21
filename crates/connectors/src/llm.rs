use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub model_used: String,
    pub usage: TokenUsage,
    pub processing_time_ms: u64,
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Generic LLM connector trait that all providers must implement
#[async_trait]
pub trait LlmConnector: Send + Sync {
    /// Unique identifier for this connector
    fn provider_name(&self) -> &'static str;

    /// Check if the connector is healthy and ready to process requests
    async fn health_check(&self) -> Result<bool>;

    /// Generate a response based on the conversation
    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse>;

    /// Get available models
    async fn available_models(&self) -> Result<Vec<String>>;

    /// Get provider-specific configuration schema
    fn config_schema(&self) -> serde_json::Value;
}
