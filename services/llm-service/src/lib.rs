use anyhow::Result;
use async_trait::async_trait;
use meshag_connectors::{ChatMessage, LlmConnector, LlmRequest, MessageRole};
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::{EventQueue, ProcessingEvent, StreamConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct LlmService {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn LlmConnector>>>>,
    default_connector: Arc<RwLock<Option<String>>>,
    conversations: Arc<RwLock<HashMap<Uuid, Vec<ChatMessage>>>>,
}

impl LlmService {
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            default_connector: Arc::new(RwLock::new(None)),
            conversations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_connector(&mut self, name: &str, connector: Arc<dyn LlmConnector>) {
        let mut connectors = self.connectors.write().await;
        connectors.insert(name.to_string(), connector);

        // Set as default if it's the first connector
        let mut default = self.default_connector.write().await;
        if default.is_none() {
            *default = Some(name.to_string());
        }
    }

    pub async fn get_connector(&self, name: Option<&str>) -> Option<Arc<dyn LlmConnector>> {
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
        info!("LLM service starting to consume from stt.output");

        let service = Arc::new(self.clone());
        let q_clone = queue.clone();
        queue
            .consume_events(
                StreamConfig::stt_output(),
                "stt.output".to_string(),
                move |event| {
                    let svc = Arc::clone(&service);
                    let q = q_clone.clone();
                    async move { handle_text_event(svc, q, event).await }
                },
            )
            .await
    }

    pub async fn add_message(&self, conversation_id: Uuid, message: ChatMessage) {
        let mut conversations = self.conversations.write().await;
        conversations
            .entry(conversation_id)
            .or_insert_with(Vec::new)
            .push(message);
    }

    pub async fn get_conversation(&self, conversation_id: Uuid) -> Vec<ChatMessage> {
        let conversations = self.conversations.read().await;
        conversations
            .get(&conversation_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn clear_conversation(&self, conversation_id: Uuid) {
        let mut conversations = self.conversations.write().await;
        conversations.remove(&conversation_id);
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

    pub async fn active_conversations(&self) -> usize {
        let conversations = self.conversations.read().await;
        conversations.len()
    }
}

impl Clone for LlmService {
    fn clone(&self) -> Self {
        Self {
            connectors: Arc::clone(&self.connectors),
            default_connector: Arc::clone(&self.default_connector),
            conversations: Arc::clone(&self.conversations),
        }
    }
}

async fn handle_text_event(
    service: Arc<LlmService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    match event.event_type.as_str() {
        "transcription_complete" => handle_transcription(service, queue, event).await?,
        "conversation_start" => handle_conversation_start(service, event).await?,
        "conversation_end" => handle_conversation_end(service, event).await?,
        _ => error!("Unknown event type: {}", event.event_type),
    }
    Ok(())
}

async fn handle_transcription(
    service: Arc<LlmService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    let text = event.payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing text in transcription payload"))?
        .to_string();

    // Add user message to conversation
    let user_message = ChatMessage {
        role: MessageRole::User,
        content: text,
        timestamp: chrono::Utc::now(),
    };
    service
        .add_message(event.conversation_id, user_message)
        .await;

    // Get connector preference from event or use default
    let connector_name = event.payload["connector"].as_str();
    let connector = match service.get_connector(connector_name).await {
        Some(c) => c,
        None => {
            warn!("No connector available for LLM processing");
            return Ok(());
        }
    };

    // Get conversation history
    let messages = service.get_conversation(event.conversation_id).await;

    // Prepare LLM request
    let llm_request = LlmRequest {
        messages,
        model: event.payload["model"].as_str().map(|s| s.to_string()),
        temperature: event.payload["temperature"].as_f64().map(|f| f as f32),
        max_tokens: event.payload["max_tokens"].as_u64().map(|u| u as u32),
        system_prompt: event.payload["system_prompt"]
            .as_str()
            .map(|s| s.to_string()),
        options: HashMap::new(),
    };

    // Generate response
    match connector.generate(llm_request).await {
        Ok(response) => {
            // Add assistant message to conversation
            let assistant_message = ChatMessage {
                role: MessageRole::Assistant,
                content: response.text.clone(),
                timestamp: chrono::Utc::now(),
            };
            service
                .add_message(event.conversation_id, assistant_message)
                .await;

            let output_event = ProcessingEvent {
                conversation_id: event.conversation_id,
                correlation_id: event.correlation_id,
                event_type: "llm_response_complete".to_string(),
                payload: json!({
                    "text": response.text,
                    "model": response.model_used,
                    "usage": response.usage,
                    "processing_time_ms": response.processing_time_ms,
                    "provider": connector.provider_name()
                }),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                source_service: "llm-service".to_string(),
                target_service: "tts-service".to_string(),
            };

            queue.publish_event("llm.output", output_event).await?;
        }
        Err(e) => {
            error!("LLM generation failed: {}", e);
        }
    }

    Ok(())
}

async fn handle_conversation_start(service: Arc<LlmService>, event: ProcessingEvent) -> Result<()> {
    // Add system message if provided
    if let Some(system_prompt) = event.payload["system_prompt"].as_str() {
        let system_message = ChatMessage {
            role: MessageRole::System,
            content: system_prompt.to_string(),
            timestamp: chrono::Utc::now(),
        };
        service
            .add_message(event.conversation_id, system_message)
            .await;
    }

    info!(conversation_id = %event.conversation_id, "LLM conversation started");
    Ok(())
}

async fn handle_conversation_end(service: Arc<LlmService>, event: ProcessingEvent) -> Result<()> {
    service.clear_conversation(event.conversation_id).await;
    info!(conversation_id = %event.conversation_id, "LLM conversation ended");
    Ok(())
}

// Service state for HTTP handlers
pub struct LlmServiceState {
    pub event_queue: EventQueue,
    pub llm_service: LlmService,
}

#[async_trait]
impl ServiceState for LlmServiceState {
    fn service_name(&self) -> String {
        "llm-service".to_string()
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
        let connector_health = self.llm_service.health_check_connectors().await;
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

        let connectors = self.llm_service.list_connectors().await;
        let connector_health = self.llm_service.health_check_connectors().await;
        let active_conversations = self.llm_service.active_conversations().await;

        metrics.push(format!("active_connectors {}", connectors.len()));
        metrics.push(format!(
            "healthy_connectors {}",
            connector_health.values().filter(|&&h| h).count()
        ));
        metrics.push(format!("active_conversations {}", active_conversations));

        // Add queue metrics
        if let Ok(queue_metrics) = self.event_queue.get_metrics("stt.output").await {
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
