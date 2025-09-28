use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum_macros::EnumString;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttRequest {
    pub audio_data: Vec<u8>,
    pub language: Option<String>,
    pub sample_rate: u32,
    pub channels: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, EnumString, PartialEq)]
pub enum AudioFormat {
    #[strum(serialize = "pcm")]
    Wav,
    #[strum(serialize = "audio/x-mulaw")]
    Mulaw,
}

#[async_trait]
pub trait SttConnector: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn health_check(&self) -> Result<bool>;

    fn supported_formats(&self) -> Vec<AudioFormat>;

    fn supported_languages(&self) -> Option<Vec<String>>;

    fn config_schema(&self) -> serde_json::Value;

    fn as_any(&self) -> &dyn std::any::Any;
}
