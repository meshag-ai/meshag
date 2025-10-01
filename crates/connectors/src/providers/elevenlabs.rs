use crate::tts::{AudioFormat, TtsConnector, TtsRequest, TtsResponse, Voice};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

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
        let url = format!("{}/user", self.config.base_url);
        let response = self
            .client
            .get(&url)
            .header("xi-api-key", &self.config.api_key)
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse> {
        let start_time = std::time::Instant::now();

        let voice_id = request
            .voice_id
            .as_ref()
            .unwrap_or(&self.config.default_voice_id);

        let output_format = match request.format {
            AudioFormat::Mulaw => "ulaw_8000",
            _ => "ulaw_8000",
        };

        let mut payload = json!({
            "text": request.text,
            "model_id": "eleven_multilingual_v2"
        });

        let mut voice_settings = json!({});
        if let Some(speed) = request.speed {
            voice_settings["speed"] = json!(speed.clamp(0.25, 4.0));
        }
        if let Some(pitch) = request.pitch {
            voice_settings["stability"] = json!((pitch + 1.0) / 2.0);
        }

        if voice_settings.as_object().unwrap().is_empty() {
            voice_settings = json!({
                "stability": 0.5,
                "similarity_boost": 0.75,
                "style": 0.0,
                "use_speaker_boost": true
            });
        }

        payload["voice_settings"] = voice_settings;

        let url = format!(
            "{}/text-to-speech/{}?output_format={}",
            self.config.base_url, voice_id, output_format
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("xi-api-key", &self.config.api_key)
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "ElevenLabs API error {}: {}",
                status,
                error_text
            ));
        }

        let audio_data = response.bytes().await?.to_vec();
        let processing_time = start_time.elapsed().as_millis() as u64;

        let (sample_rate, channels) = match request.format {
            AudioFormat::Mulaw => (8000, 1), // μ-law 8kHz mono
            _ => (8000, 1),
        };

        let duration_ms = if !audio_data.is_empty() {
            (audio_data.len() as u64 * 1000) / (sample_rate as u64 * channels as u64)
        } else {
            0
        };

        let mut provider_metadata = std::collections::HashMap::new();
        provider_metadata.insert("voice_id".to_string(), json!(voice_id));
        provider_metadata.insert("model_id".to_string(), json!("eleven_multilingual_v2"));
        provider_metadata.insert("output_format".to_string(), json!(output_format));

        Ok(TtsResponse {
            audio_data,
            format: request.format,
            sample_rate,
            channels,
            duration_ms,
            processing_time_ms: processing_time,
            provider_metadata,
        })
    }

    async fn available_voices(&self) -> Result<Vec<Voice>> {
        let url = format!("{}/voices", self.config.base_url);
        let response = self
            .client
            .get(&url)
            .header("xi-api-key", &self.config.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(vec![
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
                Voice {
                    id: "EXAVITQu4vr4xnSDxMaL".to_string(),
                    name: "Bella".to_string(),
                    language: "en-US".to_string(),
                    gender: Some("female".to_string()),
                    description: Some("Soft American female voice".to_string()),
                },
            ]);
        }

        let voices_response: serde_json::Value = response.json().await?;
        let mut voices = Vec::new();

        if let Some(voices_array) = voices_response["voices"].as_array() {
            for voice_data in voices_array {
                if let (Some(voice_id), Some(name)) =
                    (voice_data["voice_id"].as_str(), voice_data["name"].as_str())
                {
                    let labels = voice_data["labels"].as_object();
                    let gender = labels
                        .and_then(|l| l.get("gender"))
                        .and_then(|g| g.as_str())
                        .map(|s| s.to_string());

                    let language = labels
                        .and_then(|l| l.get("accent"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("en-US");

                    let description = voice_data["description"].as_str().map(|s| s.to_string());

                    voices.push(Voice {
                        id: voice_id.to_string(),
                        name: name.to_string(),
                        language: language.to_string(),
                        gender,
                        description,
                    });
                }
            }
        }

        if voices.is_empty() {
            voices.push(Voice {
                id: "21m00Tcm4TlvDq8ikWAM".to_string(),
                name: "Rachel".to_string(),
                language: "en-US".to_string(),
                gender: Some("female".to_string()),
                description: Some("Young American female voice".to_string()),
            });
        }

        Ok(voices)
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Mulaw]
    }
}

pub struct ElevenLabs;

impl ElevenLabs {
    pub fn tts_connector(config: ElevenLabsConfig) -> Arc<dyn TtsConnector> {
        Arc::new(ElevenLabsTtsConnector::new(config))
    }
}
