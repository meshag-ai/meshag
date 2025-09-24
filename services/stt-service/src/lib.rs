use anyhow::Result;
use async_trait::async_trait;
use meshag_connectors::{AudioFormat, SttConnector, SttRequest};
use meshag_orchestrator::ServiceRouter;
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::{EventQueue, ProcessingEvent, StreamConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

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
        info!("STT service starting to consume from audio.input");

        let service = Arc::new(self.clone());
        let q_clone = queue.clone();
        queue
            .consume_events(
                StreamConfig::audio_input(),
                "audio-input".to_string(),
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
        "audio_chunk" => handle_audio_chunk(service, queue, event).await?,
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
    let audio_data = event.payload["audio_data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing audio_data in payload"))?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect::<Vec<u8>>();

    let is_final = event.payload["is_final"].as_bool().unwrap_or(false);

    if !is_final {
        // For streaming, we might want to buffer chunks
        // For now, we'll process each chunk immediately
        return Ok(());
    }

    // Get connector preference from event or use default
    let connector_name = event.payload["connector"].as_str();

    // Clone the service to get access to connectors
    let connectors = service.connectors.read().await;
    let default_name = service.default_connector.read().await;

    let connector_key = connector_name.or_else(|| default_name.as_deref());
    let connector = match connector_key.and_then(|name| connectors.get(name)) {
        Some(c) => c.as_ref(),
        None => {
            warn!("No connector available for STT processing");
            return Ok(());
        }
    };

    // Prepare STT request
    let stt_request = SttRequest {
        audio_data,
        language: event.payload["language"].as_str().map(|s| s.to_string()),
        sample_rate: event.payload["sample_rate"].as_u64().unwrap_or(16000) as u32,
        channels: event.payload["channels"].as_u64().unwrap_or(1) as u8,
        format: AudioFormat::Wav, // Default format, could be configurable
        options: HashMap::new(),
    };

    // Transcribe audio
    match connector.transcribe(stt_request).await {
        Ok(response) => {
            let output_event = ProcessingEvent {
                conversation_id: event.conversation_id,
                correlation_id: event.correlation_id,
                event_type: "transcription_complete".to_string(),
                payload: json!({
                    "text": response.text,
                    "confidence": response.confidence,
                    "language_detected": response.language_detected,
                    "processing_time_ms": response.processing_time_ms,
                    "provider": connector.provider_name()
                }),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                source_service: "stt-service".to_string(),
                target_service: "llm-service".to_string(),
            };

            queue.publish_event("stt-output", output_event).await?;
        }
        Err(e) => {
            error!("STT transcription failed: {}", e);
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
        if let Ok(queue_metrics) = self.event_queue.get_metrics("audio-input").await {
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
