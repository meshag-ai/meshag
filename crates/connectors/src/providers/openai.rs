use crate::llm::{ChatMessage, LlmConnector, LlmRequest, LlmResponse, MessageRole, TokenUsage};
use crate::stt::{AudioFormat, SttConnector, SttRequest, SttResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// OpenAI provider configuration
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_llm_model: String,
    pub default_stt_model: String,
}

impl OpenAIConfig {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            default_llm_model: "gpt-4o-mini".to_string(),
            default_stt_model: "whisper-1".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn with_llm_model(mut self, model: String) -> Self {
        self.default_llm_model = model;
        self
    }

    pub fn with_stt_model(mut self, model: String) -> Self {
        self.default_stt_model = model;
        self
    }
}

/// OpenAI LLM Connector
#[derive(Debug, Clone)]
pub struct OpenAILlmConnector {
    config: OpenAIConfig,
    client: reqwest::Client,
}

impl OpenAILlmConnector {
    pub fn new(config: OpenAIConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|msg| {
                json!({
                    "role": match msg.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                    },
                    "content": msg.content
                })
            })
            .collect()
    }
}

#[async_trait]
impl LlmConnector for OpenAILlmConnector {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/models", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
        let start_time = Instant::now();

        let model = request
            .model
            .unwrap_or_else(|| self.config.default_llm_model.clone());
        let messages = self.convert_messages(&request.messages);

        let mut payload = json!({
            "model": model,
            "messages": messages,
        });

        if let Some(temp) = request.temperature {
            payload["temperature"] = json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = json!(max_tokens);
        }

        // Add system prompt if provided
        if let Some(system_prompt) = request.system_prompt {
            let mut msgs = vec![json!({
                "role": "system",
                "content": system_prompt
            })];
            msgs.extend(messages);
            payload["messages"] = json!(msgs);
        }

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("OpenAI API error: {}", error_text));
        }

        let response_json: serde_json::Value = response.json().await?;

        let text = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response format from OpenAI"))?
            .to_string();

        let usage = response_json["usage"].clone();
        let token_usage = TokenUsage {
            prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
        };

        let processing_time = start_time.elapsed().as_millis() as u64;

        let mut provider_metadata = HashMap::new();
        provider_metadata.insert("raw_response".to_string(), response_json);

        Ok(LlmResponse {
            text,
            model_used: model,
            usage: token_usage,
            processing_time_ms: processing_time,
            provider_metadata,
        })
    }

    async fn available_models(&self) -> Result<Vec<String>> {
        let response = self
            .client
            .get(&format!("{}/models", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch models from OpenAI"));
        }

        let models_json: serde_json::Value = response.json().await?;
        let models = models_json["data"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid models response format"))?
            .iter()
            .filter_map(|model| model["id"].as_str().map(|s| s.to_string()))
            .collect();

        Ok(models)
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "OpenAI API key"
                },
                "base_url": {
                    "type": "string",
                    "description": "Base URL for OpenAI API (optional)",
                    "default": "https://api.openai.com/v1"
                },
                "default_llm_model": {
                    "type": "string",
                    "description": "Default LLM model to use",
                    "default": "gpt-3.5-turbo"
                }
            },
            "required": ["api_key"]
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenAISttConnector {
    config: OpenAIConfig,
    client: reqwest::Client,
}

impl OpenAISttConnector {
    pub fn new(config: OpenAIConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SttConnector for OpenAISttConnector {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/models", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav]
    }

    fn supported_languages(&self) -> Option<Vec<String>> {
        // Whisper supports many languages, returning None means "all supported"
        None
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "OpenAI API key for Whisper"
                },
                "base_url": {
                    "type": "string",
                    "description": "Base URL for OpenAI API (optional)",
                    "default": "https://api.openai.com/v1"
                },
                "default_stt_model": {
                    "type": "string",
                    "description": "Whisper model to use",
                    "default": "whisper-1"
                }
            },
            "required": ["api_key"]
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Factory functions for OpenAI connectors
pub struct OpenAI;

impl OpenAI {
    /// Create OpenAI LLM connector
    pub fn llm_connector(config: OpenAIConfig) -> Arc<dyn LlmConnector> {
        Arc::new(OpenAILlmConnector::new(config))
    }

    /// Create OpenAI STT connector
    pub fn stt_connector(config: OpenAIConfig) -> Arc<dyn SttConnector> {
        Arc::new(OpenAISttConnector::new(config))
    }

    /// Create both LLM and STT connectors with shared config
    pub fn connectors(config: OpenAIConfig) -> (Arc<dyn LlmConnector>, Arc<dyn SttConnector>) {
        (
            Self::llm_connector(config.clone()),
            Self::stt_connector(config),
        )
    }
}
