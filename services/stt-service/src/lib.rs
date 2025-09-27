use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use meshag_connectors::{AudioFormat, SttConnector, SttRequest};
use meshag_orchestrator::ServiceRouter;
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::MediaEventPayload;
use meshag_shared::{EventQueue, ProcessingEvent, StreamConfig};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

pub struct SttService {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn SttConnector>>>>,
    default_connector: Arc<RwLock<Option<String>>>,
    router: Arc<RwLock<Option<ServiceRouter>>>,
}

impl SttService {
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            default_connector: Arc::new(RwLock::new(None)),
            router: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn register_connector(&mut self, name: &str, connector: Arc<dyn SttConnector>) {
        let mut connectors = self.connectors.write().await;
        connectors.insert(name.to_string(), connector);

        // Set as default if it's the first connector
        let mut default = self.default_connector.write().await;
        if default.is_none() {
            *default = Some(name.to_string());
        }
    }

    pub async fn get_connector(&self, name: Option<&str>) -> Option<Arc<dyn SttConnector>> {
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

        let connector_name = connector_name.as_str();

        connectors.get(connector_name).map(|c| c.clone())
    }

    pub async fn start_processing(&self, queue: EventQueue) -> Result<()> {
        info!("STT service starting to consume from AUDIO_INPUT");

        let service = Arc::new(self.clone());
        let q_clone = queue.clone();
        queue
            .consume_events(
                StreamConfig::audio_input(),
                "AUDIO_INPUT".to_string(),
                move |event| {
                    let svc = Arc::clone(&service);
                    let q = q_clone.clone();
                    async move { handle_audio_event(svc, q, event).await }
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

impl Clone for SttService {
    fn clone(&self) -> Self {
        Self {
            connectors: Arc::clone(&self.connectors),
            default_connector: Arc::clone(&self.default_connector),
            router: Arc::clone(&self.router),
        }
    }
}

async fn handle_audio_event(
    service: Arc<SttService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    match event.event_type.as_str() {
        "media_input" => handle_audio_chunk(service, queue, event).await?,
        "session_start" => handle_session_start(event).await?,
        "session_end" => handle_session_end(event).await?,
        _ => error!("Unknown event type: {}", event.event_type),
    }
    Ok(())
}

async fn handle_audio_chunk(
    service: Arc<SttService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    info!("Handling audio chunk: {:?}", event);

    // Parse the MediaEventPayload from the transport service
    let media_payload: MediaEventPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| anyhow::anyhow!("Failed to parse MediaEventPayload: {}", e))?;

    info!(
        "Processing audio chunk: session_id={}, call_sid={}, track={}, chunk={}",
        media_payload.session_id, media_payload.call_sid, media_payload.track, media_payload.chunk
    );

    // Decode base64 audio data
    let audio_data = STANDARD
        .decode(&media_payload.payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64 audio data: {}", e))?;

    // Get connector preference from event or use default
    let connector_name = event.payload.get("connector").and_then(|v| v.as_str());

    // Clone the service to get access to connectors
    let connectors = service.connectors.read().await;
    let default_name = service.default_connector.read().await;

    let connector_key = connector_name.or_else(|| default_name.as_deref());
    let connector = match connector_key.and_then(|name| connectors.get(name)) {
        Some(connector) => connector,
        None => {
            error!("No STT connector found for: {:?}", connector_key);
            return Err(anyhow::anyhow!("No STT connector available"));
        }
    };

    let format: Result<AudioFormat, strum::ParseError> =
        AudioFormat::from_str(&media_payload.media_format.encoding);

    let mut encoding = None;

    match format {
        Ok(format) => {
            encoding = Some(format);
        }
        _ => {
            error!("Unsupported audio format: {:?}", format);
            return Err(anyhow::anyhow!("Unsupported audio format"));
        }
    }

    // Process the audio chunk
    match connector
        .transcribe(SttRequest {
            audio_data: audio_data,
            language: None,
            sample_rate: media_payload.media_format.sample_rate,
            channels: media_payload.media_format.channels,
            format: encoding.unwrap(),
            options: HashMap::new(),
        })
        .await
    {
        Ok(transcription) => {
            info!("Transcription result: {:?}", transcription.text);

            // Create response event for LLM service
            let response_payload = serde_json::json!({
                "session_id": media_payload.session_id,
                "call_sid": media_payload.call_sid,
                "transcription": transcription,
                "timestamp": media_payload.timestamp,
                "track": media_payload.track,
                "chunk": media_payload.chunk
            });

            let response_event = ProcessingEvent {
                conversation_id: event.conversation_id,
                correlation_id: event.correlation_id,
                event_type: "transcription_output".to_string(),
                payload: response_payload,
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                source_service: "stt-service".to_string(),
                target_service: "llm-service".to_string(),
            };

            // Publish to LLM service
            if let Err(e) = queue.publish_event("STT_OUTPUT", response_event).await {
                error!("Failed to publish transcription to LLM service: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to process audio: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

async fn handle_session_start(event: ProcessingEvent) -> Result<()> {
    info!(conversation_id = %event.conversation_id, "STT session started");
    Ok(())
}

async fn handle_session_end(event: ProcessingEvent) -> Result<()> {
    info!(conversation_id = %event.conversation_id, "STT session ended");
    Ok(())
}

// Service state for HTTP handlers
pub struct SttServiceState {
    pub event_queue: EventQueue,
    pub stt_service: SttService,
}

#[async_trait]
impl ServiceState for SttServiceState {
    fn service_name(&self) -> String {
        "stt-service".to_string()
    }

    async fn is_ready(&self) -> Vec<HealthCheck> {
        let mut checks = vec![];

        // Check NATS connection
        let nats_healthy = self.event_queue.health_check().await.unwrap_or(false);
        checks.push(HealthCheck {
            name: "nats".to_string(),
            status: if nats_healthy {
                "healthy".to_string()
            } else {
                "unhealthy".to_string()
            },
            message: Some(if nats_healthy {
                "Connected".to_string()
            } else {
                "Disconnected".to_string()
            }),
        });

        // Check connectors
        let connector_health = self.stt_service.health_check_connectors().await;
        for (name, healthy) in connector_health {
            checks.push(HealthCheck {
                name: format!("connector_{}", name),
                status: if healthy {
                    "healthy".to_string()
                } else {
                    "unhealthy".to_string()
                },
                message: Some(if healthy {
                    "Ready".to_string()
                } else {
                    "Not ready".to_string()
                }),
            });
        }

        checks
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }

    async fn get_metrics(&self) -> Vec<String> {
        let mut metrics = vec![];

        let connectors = self.stt_service.list_connectors().await;
        let connector_health = self.stt_service.health_check_connectors().await;

        metrics.push(format!("active_connectors {}", connectors.len()));
        metrics.push(format!(
            "healthy_connectors {}",
            connector_health.values().filter(|&&h| h).count()
        ));

        // Add queue metrics
        if let Ok(queue_metrics) = self.event_queue.get_metrics("AUDIO_INPUT").await {
            metrics.push(format!(
                "pending_messages {}",
                queue_metrics.pending_messages
            ));
            metrics.push(format!(
                "delivered_messages {}",
                queue_metrics.delivered_messages
            ));
        }

        metrics
    }
}
