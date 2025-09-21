use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: Option<String>,
    pub language: Option<String>,
    pub speed: Option<f32>,
    pub pitch: Option<f32>,
    pub format: AudioFormat,
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsResponse {
    pub audio_data: Vec<u8>,
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u8,
    pub duration_ms: u64,
    pub processing_time_ms: u64,
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Flac,
    Ogg,
}

/// Generic TTS connector trait that all providers must implement
#[async_trait]
pub trait TtsConnector: Send + Sync {
    /// Unique identifier for this connector
    fn provider_name(&self) -> &'static str;

    /// Check if the connector is healthy and ready to process requests
    async fn health_check(&self) -> Result<bool>;

    /// Convert text to speech
    async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse>;

    /// Get available voices
    async fn available_voices(&self) -> Result<Vec<Voice>>;

    /// Get supported audio formats
    fn supported_formats(&self) -> Vec<AudioFormat>;

    /// Get provider-specific configuration schema
    fn config_schema(&self) -> serde_json::Value;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: Option<String>,
    pub description: Option<String>,
}
