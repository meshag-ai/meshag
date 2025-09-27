use crate::stt::{AudioFormat, SttConnector, SttRequest, SttResponse};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

/// Deepgram provider configuration
#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

impl DeepgramConfig {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.deepgram.com/v1".to_string(),
            default_model: "nova-2".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.default_model = model;
        self
    }
}

/// Deepgram STT Connector
#[derive(Debug, Clone)]
pub struct DeepgramSttConnector {
    config: DeepgramConfig,
    client: reqwest::Client,
}

impl DeepgramSttConnector {
    pub fn new(config: DeepgramConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SttConnector for DeepgramSttConnector {
    fn provider_name(&self) -> &'static str {
        "deepgram"
    }

    async fn health_check(&self) -> Result<bool> {
        // TODO: Implement Deepgram health check
        Ok(true)
    }

    async fn transcribe(&self, _request: SttRequest) -> Result<SttResponse> {
        // TODO: Implement Deepgram transcription
        todo!("Deepgram transcription not yet implemented")
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Mulaw]
    }

    fn supported_languages(&self) -> Option<Vec<String>> {
        None // Supports many languages
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "Deepgram API key"
                },
                "base_url": {
                    "type": "string",
                    "description": "Base URL for Deepgram API (optional)",
                    "default": "https://api.deepgram.com/v1"
                },
                "default_model": {
                    "type": "string",
                    "description": "Default Deepgram model to use",
                    "default": "nova-2"
                }
            },
            "required": ["api_key"]
        })
    }
}

/// Factory functions for Deepgram connectors
pub struct Deepgram;

impl Deepgram {
    /// Create Deepgram STT connector
    pub fn stt_connector(config: DeepgramConfig) -> Box<dyn SttConnector> {
        Box::new(DeepgramSttConnector::new(config))
    }
}
