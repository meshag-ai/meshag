use crate::stt::{AudioFormat, SttConnector, SttResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Message,
};
use url::Url;

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    #[serde(rename = "type")]
    response_type: String,
    channel: Option<DeepgramChannel>,
    metadata: Option<DeepgramMetadata>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    confidence: f32,
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct DeepgramMetadata {
    model_info: Option<DeepgramModelInfo>,
    request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepgramModelInfo {
    name: String,
    version: String,
}

#[derive(Debug)]
pub struct SessionConnection {
    pub session_id: String,
    pub write_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub transcription_rx: mpsc::UnboundedReceiver<SttResponse>,
    pub _handle: tokio::task::JoinHandle<()>,
}
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

#[derive(Debug)]
pub struct DeepgramSttConnector {
    config: DeepgramConfig,
    client: reqwest::Client,
    pub sessions: Arc<RwLock<HashMap<String, Arc<Mutex<SessionConnection>>>>>,
}

impl DeepgramSttConnector {
    pub fn new(config: DeepgramConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_encoding(&self, format: &AudioFormat) -> &'static str {
        match format {
            AudioFormat::Wav => "linear16",
            AudioFormat::Mulaw => "mulaw",
        }
    }

    pub async fn create_session(
        &self,
        session_id: String,
        sample_rate: u32,
        channels: u32,
        format: AudioFormat,
        language: Option<String>,
    ) -> Result<()> {
        let mut ws_url = format!(
            "wss://api.deepgram.com/v1/listen?model={}&encoding={}&sample_rate={}&channels={}&interim_results=true&punctuate=true&vad_events=true",
            self.config.default_model,
            self.get_encoding(&format),
            sample_rate,
            channels
        );

        if let Some(lang) = &language {
            ws_url.push_str(&format!("&language={}", lang));
        }

        println!("WebSocket URL: {}", ws_url);

        let mut url = ws_url.into_client_request()?;
        let headers = url.headers_mut();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Token {}", self.config.api_key))?,
        );

        let (ws_stream, _response) = connect_async(url)
            .await
            .map_err(|e| anyhow!("Failed to connect to Deepgram: {}", e))?;

        let (write, read) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (transcription_tx, transcription_rx) = mpsc::unbounded_channel::<SttResponse>();

        let session_id_clone = session_id.clone();

        let handle = tokio::spawn(async move {
            let mut write = write;
            let mut read = read;

            let write_task = tokio::spawn(async move {
                while let Some(audio_data) = write_rx.recv().await {
                    if let Err(e) = write.send(Message::Binary(audio_data)).await {
                        tracing::error!("Failed to send audio data to Deepgram: {}", e);
                        break;
                    }
                }
            });

            let read_task = tokio::spawn(async move {
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(response) = serde_json::from_str::<DeepgramResponse>(&text) {
                                match response.response_type.as_str() {
                                    "Results" => {
                                        if let Some(channel) = response.channel {
                                            if let Some(alternative) = channel.alternatives.first()
                                            {
                                                let mut provider_metadata = HashMap::new();

                                                if let Some(metadata) = response.metadata {
                                                    if let Some(model_info) = metadata.model_info {
                                                        provider_metadata.insert(
                                                            "model_name".to_string(),
                                                            json!(model_info.name),
                                                        );
                                                        provider_metadata.insert(
                                                            "model_version".to_string(),
                                                            json!(model_info.version),
                                                        );
                                                    }
                                                    if let Some(request_id) = metadata.request_id {
                                                        provider_metadata.insert(
                                                            "request_id".to_string(),
                                                            json!(request_id),
                                                        );
                                                    }
                                                }

                                                let stt_response = SttResponse {
                                                    text: alternative.transcript.clone(),
                                                    confidence: Some(alternative.confidence),
                                                    language_detected: None,
                                                    processing_time_ms: 0,
                                                    provider_metadata,
                                                };

                                                if let Err(e) = transcription_tx.send(stt_response)
                                                {
                                                    tracing::error!(
                                                        "Failed to send transcription: {}",
                                                        e
                                                    );
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    "Metadata" => {}
                                    _ => {
                                        tracing::debug!(
                                            "Received response type: {}",
                                            response.response_type
                                        );
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            tracing::info!("WebSocket closed for session: {}", session_id_clone);
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "WebSocket error for session {}: {}",
                                session_id_clone,
                                e
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            });

            tokio::select! {
                _ = write_task => {},
                _ = read_task => {},
            }
        });

        let session_conn = SessionConnection {
            session_id: session_id.clone(),
            write_tx,
            transcription_rx,
            _handle: handle,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, Arc::new(Mutex::new(session_conn)));

        Ok(())
    }

    pub async fn send_audio(&self, session_id: &str, audio_data: Vec<u8>) -> Result<()> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let session = session.lock().await;
            session
                .write_tx
                .send(audio_data)
                .map_err(|e| anyhow!("Failed to send audio data: {}", e))?;
            Ok(())
        } else {
            Err(anyhow!("Session not found: {}", session_id))
        }
    }

    pub async fn get_transcription(&self, session_id: &str) -> Result<Option<SttResponse>> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let mut session = session.lock().await;
            Ok(session.transcription_rx.try_recv().ok())
        } else {
            Err(anyhow!("Session not found: {}", session_id))
        }
    }

    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(session_id) {
            let session = session.lock().await;
            session._handle.abort();
            tracing::info!("Closed session: {}", session_id);
            Ok(())
        } else {
            Err(anyhow!("Session not found: {}", session_id))
        }
    }
}

impl Clone for DeepgramSttConnector {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
            sessions: Arc::clone(&self.sessions),
        }
    }
}

#[async_trait]
impl SttConnector for DeepgramSttConnector {
    fn provider_name(&self) -> &'static str {
        "deepgram"
    }

    async fn health_check(&self) -> Result<bool> {
        let ws_url = format!(
            "wss://api.deepgram.com/v1/listen?model={}",
            self.config.default_model
        );
        let url = Url::parse(&ws_url).map_err(|e| anyhow!("Invalid WebSocket URL: {}", e))?;

        match connect_async(url).await {
            Ok((_ws_stream, _response)) => Ok(true),
            Err(e) => {
                tracing::warn!("Deepgram health check failed: {}", e);
                Ok(false)
            }
        }
    }

    fn supported_formats(&self) -> Vec<AudioFormat> {
        vec![AudioFormat::Wav, AudioFormat::Mulaw]
    }

    fn supported_languages(&self) -> Option<Vec<String>> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct Deepgram;

impl Deepgram {
    /// Create Deepgram STT connector
    pub fn stt_connector(config: DeepgramConfig) -> Arc<dyn SttConnector> {
        Arc::new(DeepgramSttConnector::new(config))
    }
}
