use crate::tts::{AudioFormat, TtsConnector, TtsRequest, TtsResponse, Voice};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

/// ElevenLabs provider configuration
#[derive(Debug, Clone)]
pub struct ElevenLabsConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_voice_id: String,
}

impl ElevenLabsConfig {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.elevenlabs.io/v1".to_string(),
            default_voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(), // Rachel voice
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn with_voice(mut self, voice_id: String) -> Self {
        self.default_voice_id = voice_id;
        self
    }
}

/// ElevenLabs TTS Connector
#[derive(Debug, Clone)]
pub struct ElevenLabsTtsConnector {
    config: ElevenLabsConfig,
    client: reqwest::Client,
}

impl ElevenLabsTtsConnector {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl TtsConnector for ElevenLabsTtsConnector {
    fn provider_name(&self) -> &'static str {
        "elevenlabs"
    }

    async fn health_check(&self) -> Result<bool> {
        // TODO: Implement ElevenLabs health check
        Ok(true)
    }

    async fn synthesize(&self, _request: TtsRequest) -> Result<TtsResponse> {
        // TODO: Implement ElevenLabs synthesis
        todo!("ElevenLabs synthesis not yet implemented")
    }

    async fn available_voices(&self) -> Result<Vec<Voice>> {
        // TODO: Implement ElevenLabs voices listing
        Ok(vec![
            Voice {
                id: "21m00Tcm4TlvDq8ikWAM".to_string(),
                name: "Rachel".to_string(),
                language: "en-US".to_string(),
                gender: Some("female".to_string()),
                description: Some("Young American female voice".to_string()),
            },
            Voice {
                id: "AZnzlk1XvdvUeBnXmlld".to_string(),
                name: "Domi".to_string(),
                language: "en-US".to_string(),
                gender: Some("female".to_string()),
                description: Some("Strong American female voice".to_string()),
            },
        ])
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Mp3, AudioFormat::Wav]
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "ElevenLabs API key"
                },
                "base_url": {
                    "type": "string",
                    "description": "Base URL for ElevenLabs API (optional)",
                    "default": "https://api.elevenlabs.io/v1"
                },
                "default_voice_id": {
                    "type": "string",
                    "description": "Default voice ID to use",
                    "default": "21m00Tcm4TlvDq8ikWAM"
                }
            },
            "required": ["api_key"]
        })
    }
}

/// Factory functions for ElevenLabs connectors
pub struct ElevenLabs;

impl ElevenLabs {
    /// Create ElevenLabs TTS connector
    pub fn tts_connector(config: ElevenLabsConfig) -> Arc<dyn TtsConnector> {
        Arc::new(ElevenLabsTtsConnector::new(config))
    }
}
