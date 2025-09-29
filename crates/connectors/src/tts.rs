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
    Mulaw,
}

#[async_trait]
pub trait TtsConnector: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn health_check(&self) -> Result<bool>;

    async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse>;

    async fn available_voices(&self) -> Result<Vec<Voice>>;

    fn supported_formats(&self) -> Vec<AudioFormat>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: Option<String>,
    pub description: Option<String>,
}
