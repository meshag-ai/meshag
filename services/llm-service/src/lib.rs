use anyhow::Result;
use async_trait::async_trait;
use meshag_connectors::{ChatMessage, LlmConnector, LlmRequest, MessageRole};
use meshag_service_common::ServiceState;
use meshag_shared::{EventQueue, ProcessingEvent, StreamConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct LlmService {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn LlmConnector>>>>,
    default_connector: Arc<RwLock<Option<String>>>,
    conversations: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>,
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
        info!("LLM service starting to consume from STT_OUTPUT");

        let service = Arc::new(self.clone());
        let q_clone = queue.clone();
        let subject = "STT_OUTPUT.session.*";
        queue
            .consume_events(
                StreamConfig::stt_output(),
                subject.to_string(),
                move |event| {
                    let svc = Arc::clone(&service);
                    let q = q_clone.clone();
                    async move { handle_text_event(svc, q, event).await }
                },
            )
            .await
    }

    pub async fn add_message(&self, session_id: String, message: ChatMessage) {
        let mut conversations = self.conversations.write().await;
        conversations
            .entry(session_id.clone())
            .or_insert_with(Vec::new)
            .push(message);
    }

    pub async fn get_conversation(&self, session_id: String) -> Vec<ChatMessage> {
        let conversations = self.conversations.read().await;
        conversations.get(&session_id).cloned().unwrap_or_default()
    }

    pub async fn clear_conversation(&self, session_id: String) {
        let mut conversations = self.conversations.write().await;
        conversations.remove(&session_id);
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
        "transcription_output" => handle_transcription(service, queue, event).await?,
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
    let text = event.payload["transcription"]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing text in transcription payload"))?
        .to_string();

    let user_message = ChatMessage {
        role: MessageRole::User,
        content: text,
        timestamp: chrono::Utc::now(),
    };
    service
        .add_message(event.session_id.clone(), user_message)
        .await;

    let connector_name = event.payload["connector"].as_str();
    let connector = match service.get_connector(connector_name).await {
        Some(c) => c,
        None => {
            warn!("No connector available for LLM processing");
            return Ok(());
        }
    };

    let messages = service.get_conversation(event.session_id.clone()).await;

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

    match connector.generate(llm_request).await {
        Ok(response) => {
            let assistant_message = ChatMessage {
                role: MessageRole::Assistant,
                content: response.text.clone(),
                timestamp: chrono::Utc::now(),
            };
            println!("content: {}", response.text);
            service
                .add_message(event.session_id.clone(), assistant_message)
                .await;

            let session_id = event.session_id.clone();
            let output_event = ProcessingEvent {
                session_id: session_id.clone(),
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
            let subject = format!("LLM_OUTPUT.session.{}", session_id);
            queue.publish_event(&subject, output_event).await?;
        }
        Err(e) => {
            error!("LLM generation failed: {}", e);
        }
    }

    Ok(())
}

async fn handle_conversation_end(service: Arc<LlmService>, event: ProcessingEvent) -> Result<()> {
    service.clear_conversation(event.session_id.clone()).await;
    info!(session_id = %event.session_id, "LLM conversation ended");
    Ok(())
}

pub struct LlmServiceState {
    pub event_queue: EventQueue,
    pub llm_service: LlmService,
}

#[async_trait]
impl ServiceState for LlmServiceState {
    fn service_name(&self) -> String {
        "llm-service".to_string()
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }
}
