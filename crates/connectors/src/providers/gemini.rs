use crate::llm::{ChatMessage, LlmConnector, LlmRequest, LlmResponse, MessageRole, TokenUsage};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_llm_model: String,
}

impl GeminiConfig {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            default_llm_model: "gemini-pro".to_string(),
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
}

#[derive(Debug, Clone)]
pub struct GeminiLlmConnector {
    config: GeminiConfig,
    client: reqwest::Client,
}

impl GeminiLlmConnector {
    pub fn new(config: GeminiConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|msg| {
                // Gemini uses "user" and "model" roles, and "parts" with "text" content
                let role = match msg.role {
                    MessageRole::System => "user", // Gemini doesn't have system role, we'll prepend it
                    MessageRole::User => "user",
                    MessageRole::Assistant => "model",
                };
                json!({
                    "role": role,
                    "parts": [{
                        "text": msg.content
                    }]
                })
            })
            .collect()
    }
}

#[async_trait]
impl LlmConnector for GeminiLlmConnector {
    fn provider_name(&self) -> &'static str {
        "gemini"
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if we can list models
        let response = self
            .client
            .get(&format!("{}/models?key={}", self.config.base_url, self.config.api_key))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    async fn generate(&self, request: LlmRequest) -> Result<LlmResponse> {
        let start_time = Instant::now();

        let model = request
            .model
            .unwrap_or_else(|| self.config.default_llm_model.clone());
        
        let mut contents = self.convert_messages(&request.messages);

        // Handle system prompt - Gemini doesn't support system role directly,
        // so we prepend it as a user message
        if let Some(system_prompt) = request.system_prompt {
            contents.insert(0, json!({
                "role": "user",
                "parts": [{
                    "text": system_prompt
                }]
            }));
        }

        let mut payload = json!({
            "contents": contents,
        });

        // Gemini uses generationConfig for parameters
        let mut generation_config = json!({});
        
        if let Some(temp) = request.temperature {
            generation_config["temperature"] = json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            generation_config["maxOutputTokens"] = json!(max_tokens);
        }

        if !generation_config.as_object().unwrap().is_empty() {
            payload["generationConfig"] = generation_config;
        }

        // Add any additional options
        for (key, value) in request.options {
            payload[key] = value;
        }

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.config.base_url,
            model,
            self.config.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Gemini API error: {}", error_text));
        }

        let response_json: serde_json::Value = response.json().await?;

        // Extract text from Gemini response format
        let text = response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response format from Gemini"))?
            .to_string();

        // Extract token usage if available
        let usage_info = response_json["usageMetadata"].clone();
        let token_usage = TokenUsage {
            prompt_tokens: usage_info["promptTokenCount"]
                .as_u64()
                .unwrap_or(0) as u32,
            completion_tokens: usage_info["candidatesTokenCount"]
                .as_u64()
                .unwrap_or(0) as u32,
            total_tokens: usage_info["totalTokenCount"]
                .as_u64()
                .unwrap_or(0) as u32,
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
            .get(&format!("{}/models?key={}", self.config.base_url, self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch models from Gemini"));
        }

        let models_json: serde_json::Value = response.json().await?;
        let models = models_json["models"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid models response format"))?
            .iter()
            .filter_map(|model| {
                let name = model["name"].as_str()?;
                // Extract model name from full path like "models/gemini-pro"
                name.strip_prefix("models/").map(|s| s.to_string())
            })
            .collect();

        Ok(models)
    }
}

/// Factory functions for Gemini connectors
pub struct Gemini;

impl Gemini {
    /// Create Gemini LLM connector
    pub fn llm_connector(config: GeminiConfig) -> Arc<dyn LlmConnector> {
        Arc::new(GeminiLlmConnector::new(config))
    }
}

