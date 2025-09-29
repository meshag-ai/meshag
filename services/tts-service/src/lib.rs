use anyhow::Result;
use async_trait::async_trait;
use meshag_connectors::{TtsConnector, TtsRequest};
use meshag_service_common::ServiceState;
use meshag_shared::{EventQueue, ProcessingEvent, StreamConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct TtsService {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn TtsConnector>>>>,
    default_connector: Arc<RwLock<Option<String>>>,
}

impl TtsService {
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            default_connector: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn register_connector(&mut self, name: &str, connector: Arc<dyn TtsConnector>) {
        let mut connectors = self.connectors.write().await;
        connectors.insert(name.to_string(), connector);

        let mut default = self.default_connector.write().await;
        if default.is_none() {
            *default = Some(name.to_string());
        }
    }

    pub async fn get_connector(&self, name: Option<&str>) -> Option<Arc<dyn TtsConnector>> {
        let connectors = self.connectors.read().await;

        let connector_name = match name {
            Some(n) => n.to_string(),
            None => {
                let default = self.default_connector.read().await;
                match default.as_ref() {
                    Some(name) => name.clone(),
                    None => return None,
                }
            }
        };

        connectors.get(&connector_name).map(|c| c.clone())
    }

    pub async fn start_processing(&self, queue: EventQueue) -> Result<()> {
        info!("TTS service starting to consume from LLM_OUTPUT");

        let service = Arc::new(self.clone());
        let q_clone = queue.clone();
        queue
            .consume_events(
                StreamConfig::llm_output(),
                "LLM_OUTPUT.session.*".to_string(),
                move |event| {
                    let svc = Arc::clone(&service);
                    let q = q_clone.clone();
                    async move { handle_llm_event(svc, q, event).await }
                },
            )
            .await
    }

    pub async fn list_connectors(&self) -> Vec<String> {
        let connectors = self.connectors.read().await;
        connectors.keys().cloned().collect()
    }

    pub async fn health_check_connectors(&self) -> HashMap<String, bool> {
        let connectors = self.connectors.read().await;
        let mut results = HashMap::new();

        for (name, connector) in connectors.iter() {
            let health = connector.health_check().await.unwrap_or(false);
            results.insert(name.clone(), health);
        }

        results
    }
}

impl Clone for TtsService {
    fn clone(&self) -> Self {
        Self {
            connectors: Arc::clone(&self.connectors),
            default_connector: Arc::clone(&self.default_connector),
        }
    }
}

async fn handle_llm_event(
    service: Arc<TtsService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    match event.event_type.as_str() {
        "llm_response_complete" => handle_llm_response(service, queue, event).await?,
        _ => error!("Unknown event type: {}", event.event_type),
    }
    Ok(())
}

async fn handle_llm_response(
    service: Arc<TtsService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    let text = event.payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing text in LLM response payload"))?
        .to_string();

    let connector_name = event.payload["connector"].as_str();
    let connector = match service.get_connector(connector_name).await {
        Some(c) => c,
        None => {
            warn!("No connector available for TTS processing");
            return Ok(());
        }
    };

    let tts_request = TtsRequest {
        text,
        voice_id: event.payload["voice_id"].as_str().map(|s| s.to_string()),
        language: event.payload["language"].as_str().map(|s| s.to_string()),
        speed: event.payload["speed"].as_f64().map(|f| f as f32),
        pitch: event.payload["pitch"].as_f64().map(|f| f as f32),
        format: meshag_connectors::tts::AudioFormat::Mulaw,
        options: HashMap::new(),
    };

    match connector.synthesize(tts_request).await {
        Ok(response) => {
            let session_id = event.session_id.clone();
            let output_event = ProcessingEvent {
                session_id: session_id.clone(),
                conversation_id: event.conversation_id,
                correlation_id: event.correlation_id,
                event_type: "tts_synthesis_complete".to_string(),
                payload: json!({
                    "audio_data": response.audio_data,
                    "format": response.format,
                    "sample_rate": response.sample_rate,
                    "channels": response.channels,
                    "duration_ms": response.duration_ms,
                    "processing_time_ms": response.processing_time_ms,
                    "provider": connector.provider_name()
                }),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                source_service: "tts-service".to_string(),
                target_service: "transport-service".to_string(),
            };

            info!(
                session_id = %session_id,
                duration_ms = response.duration_ms,
                "Publishing TTS output event"
            );

            let subject = format!("TTS_OUTPUT.session.{}", session_id);
            queue.publish_nats_core(&subject, output_event).await?;
        }
        Err(e) => {
            error!("TTS synthesis failed: {}", e);
        }
    }

    Ok(())
}

pub struct TtsServiceState {
    pub event_queue: EventQueue,
    pub tts_service: TtsService,
}

#[async_trait]
impl ServiceState for TtsServiceState {
    fn service_name(&self) -> String {
        "tts-service".to_string()
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }
}
