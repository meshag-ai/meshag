use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttRequest {
    pub audio_data: Vec<u8>,
    pub language: Option<String>,
    pub sample_rate: u32,
    pub channels: u8,
    pub format: AudioFormat,
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttResponse {
    pub text: String,
    pub confidence: Option<f32>,
    pub language_detected: Option<String>,
    pub processing_time_ms: u64,
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioFormat {
    Wav,
    Mp3,
    Flac,
    Ogg,
    Raw,
}

/// Generic STT connector trait that all providers must implement
#[async_trait]
pub trait SttConnector: Send + Sync {
    /// Unique identifier for this connector
    fn provider_name(&self) -> &'static str;

    /// Check if the connector is healthy and ready to process requests
    async fn health_check(&self) -> Result<bool>;

    /// Transcribe audio data to text
    async fn transcribe(&self, request: SttRequest) -> Result<SttResponse>;

    /// Get supported audio formats
    fn supported_formats(&self) -> Vec<AudioFormat>;

    /// Get supported languages (None means all languages supported)
    fn supported_languages(&self) -> Option<Vec<String>>;

    /// Get provider-specific configuration schema
    fn config_schema(&self) -> serde_json::Value;
}
