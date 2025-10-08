use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use meshag_connectors::tts::AudioFormat;
use meshag_connectors::{
    twilio_transport_connector, ParticipantConfig, RoomConfig, SessionInfo, TransportConnector,
    TransportRequest, TransportResponse, TwilioConfig,
};
use meshag_orchestrator::{AgentConfig, ConfigStorage};
use meshag_service_common::ServiceState;
use meshag_shared::{
    EventQueue, MediaEventPayload, ProcessingEvent, SubjectName, TwilioMediaData,
    TwilioMediaFormat, TwilioStartData,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransportServiceConfig {
    pub twilio_account_sid: String,
    pub twilio_auth_token: String,
    pub twilio_phone_number: String,
    pub twilio_webhook_url: String,
    pub nats_url: String,
    pub valkey_url: String,
}

impl TransportServiceConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            twilio_account_sid: std::env::var("TWILIO_ACCOUNT_SID")
                .unwrap_or_else(|_| "".to_string()),
            twilio_auth_token: std::env::var("TWILIO_AUTH_TOKEN")
                .unwrap_or_else(|_| "".to_string()),
            twilio_phone_number: std::env::var("TWILIO_PHONE_NUMBER")
                .unwrap_or_else(|_| "".to_string()),
            twilio_webhook_url: std::env::var("TWILIO_WEBHOOK_URL")
                .unwrap_or_else(|_| "".to_string()),
            nats_url: std::env::var("NATS_URL")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            valkey_url: std::env::var("VALKEY_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
        })
    }
}

#[derive(Clone)]
pub struct TransportServiceState {
    pub config: TransportServiceConfig,
    pub event_queue: Arc<EventQueue>,
    pub connectors: DashMap<String, Arc<dyn TransportConnector>>,
    pub sessions: DashMap<String, SessionInfo>,
    pub session_providers: DashMap<String, String>, // Maps session_id -> provider_name
    pub config_storage: Arc<ConfigStorage>,
}

impl TransportServiceState {
    pub async fn new(config: TransportServiceConfig) -> Result<Arc<Self>> {
        let event_queue = Arc::new(EventQueue::new("transport-service").await?);
        let connectors = DashMap::new();
        let sessions = DashMap::new();
        let valkey_url =
            std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        tracing::info!("Connecting to Valkey at: {}", valkey_url);
        let config_storage = Arc::new(ConfigStorage::new(&valkey_url).await.map_err(|e| {
            tracing::error!("Failed to connect to Valkey at '{}': {}", valkey_url, e);
            anyhow::anyhow!("Valkey connection failed: {}", e)
        })?);
        tracing::info!("Successfully connected to Valkey");

        if !config.twilio_account_sid.is_empty()
            && !config.twilio_auth_token.is_empty()
            && !config.twilio_phone_number.is_empty()
        {
            let twilio_config = TwilioConfig::new(
                config.twilio_account_sid.clone(),
                config.twilio_auth_token.clone(),
                config.twilio_phone_number.clone(),
            )
            .with_webhook_url(config.twilio_webhook_url.clone());

            let twilio_connector = twilio_transport_connector(twilio_config);
            connectors.insert("twilio".to_string(), twilio_connector);
            tracing::info!("Twilio connector registered successfully");
        } else {
            tracing::warn!(
                "Twilio credentials not provided, skipping Twilio connector registration"
            );
        }

        Ok(Arc::new(Self {
            config,
            event_queue,
            connectors,
            sessions,
            session_providers: DashMap::new(),
            config_storage,
        }))
    }

    pub fn get_connector(&self, provider: &str) -> Option<Arc<dyn TransportConnector>> {
        self.connectors.get(provider).map(|entry| entry.clone())
    }

    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<TransportResponse> {
        let connector = self
            .get_connector(&request.provider)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", request.provider))?;

        let session_id = Uuid::new_v4().to_string();

        let transport_request = TransportRequest {
            session_id: session_id.clone(),
            room_config: request.room_config,
            participant_config: request.participant_config,
            options: request.options,
        };

        let response = connector.create_session(transport_request).await?;

        let session_info = SessionInfo {
            session_id: session_id.clone(),
            room_name: response.room_name.clone(),
            room_url: response.room_url.clone(),
            status: meshag_connectors::SessionStatus::Created,
            participants: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.sessions.insert(session_id.clone(), session_info);

        self.session_providers
            .insert(session_id, request.provider.clone());

        Ok(response)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<SessionInfo> {
        if let Some(session) = self.sessions.get(session_id) {
            if let Some(provider_name) = self.get_provider_for_session(session_id) {
                if let Some(connector) = self.get_connector(&provider_name) {
                    if let Ok(updated_session) = connector.get_session(session_id).await {
                        self.sessions
                            .insert(session_id.to_string(), updated_session.clone());
                        return Ok(updated_session);
                    }
                }
            }
            Ok(session.clone())
        } else {
            Err(anyhow::anyhow!("Session '{}' not found", session_id))
        }
    }

    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                connector.end_session(session_id).await?;
            }
        }

        self.sessions.remove(session_id);
        self.session_providers.remove(session_id);

        if let Err(e) = self.delete_config(session_id).await {
            tracing::warn!("Failed to delete config for session {}: {}", session_id, e);
        }

        Ok(())
    }

    pub async fn join_as_ai_agent(&self, session_id: &str, agent_name: &str) -> Result<()> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                let agent_participant_config = meshag_connectors::ParticipantConfig {
                    name: Some(agent_name.to_string()),
                    is_owner: false,
                    permissions: meshag_connectors::ParticipantPermissions {
                        can_admin: false,
                        can_send_video: false,
                        can_send_audio: true,
                        can_send_screen_video: false,
                        can_send_screen_audio: false,
                    },
                };

                let agent_token = connector
                    .create_meeting_token(session_id, agent_participant_config.clone())
                    .await?;

                self.start_ai_agent_pipeline(session_id, &agent_token, &provider_name)
                    .await?;

                tracing::info!(
                    "AI agent joined session {} and started processing pipeline",
                    session_id
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Connector not found for provider: {}",
                    provider_name
                ))
            }
        } else {
            Err(anyhow::anyhow!(
                "Provider not found for session: {}",
                session_id
            ))
        }
    }

    async fn start_ai_agent_pipeline(
        &self,
        session_id: &str,
        agent_token: &str,
        provider_name: &str,
    ) -> Result<()> {
        let _config = self
            .get_config(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No configuration found for session: {}", session_id))?;

        let mut router = meshag_orchestrator::ServiceRouter::new(
            &self.config.valkey_url,
            (*self.event_queue).clone(),
            "transport".to_string(),
        )
        .await?;

        router.load_config(session_id).await?;

        match provider_name {
            "daily" => {
                tracing::info!(
                    "Daily.co provider detected - connector handles WebSocket internally"
                );
            }
            "twilio" => {
                tracing::info!("Twilio provider detected - audio processing via webhooks");
            }
            _ => {
                tracing::warn!(
                    "Unknown provider: {} - no specific audio handling implemented",
                    provider_name
                );
            }
        }

        self.send_ai_greeting(session_id, agent_token, &router)
            .await?;

        Ok(())
    }

    async fn send_ai_greeting(
        &self,
        session_id: &str,
        _agent_token: &str,
        router: &meshag_orchestrator::ServiceRouter,
    ) -> Result<()> {
        tracing::info!("AI agent sending greeting for session: {}", session_id);

        let greeting_text = "Hello! I'm your AI assistant. How can I help you today?";

        if let Err(e) = router
            .route_to_next(serde_json::json!({"text": greeting_text}), session_id)
            .await
        {
            tracing::error!("Failed to send greeting: {}", e);
        }

        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let sessions: Vec<SessionInfo> = self
            .sessions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(sessions)
    }

    fn get_provider_for_session(&self, session_id: &str) -> Option<String> {
        if let Some(provider) = self.session_providers.get(session_id) {
            Some(provider.clone())
        } else {
            None
        }
    }

    pub async fn store_config(&self, session_id: &str, config: &AgentConfig) -> Result<()> {
        self.config_storage.store_config(session_id, config).await
    }

    pub async fn get_config(&self, session_id: &str) -> Result<Option<AgentConfig>> {
        self.config_storage.get_config(session_id).await
    }

    pub async fn delete_config(&self, session_id: &str) -> Result<()> {
        self.config_storage.delete_config(session_id).await
    }

    pub async fn list_configs(&self) -> Result<Vec<String>> {
        self.config_storage.list_configs().await
    }

    pub async fn config_exists(&self, session_id: &str) -> Result<bool> {
        self.config_storage.config_exists(session_id).await
    }

    pub fn get_session_stats(&self) -> SessionStats {
        let total_sessions = self.sessions.len();
        let active_sessions = self
            .sessions
            .iter()
            .filter(|entry| {
                matches!(
                    entry.value().status,
                    meshag_connectors::SessionStatus::Created
                        | meshag_connectors::SessionStatus::Active
                )
            })
            .count();

        let mut provider_counts = std::collections::HashMap::new();
        for provider_entry in self.session_providers.iter() {
            *provider_counts
                .entry(provider_entry.value().clone())
                .or_insert(0) += 1;
        }

        SessionStats {
            total_sessions,
            active_sessions,
            provider_counts,
        }
    }

    pub fn session_exists(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub fn get_session_ids(&self) -> Vec<String> {
        self.sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    pub fn get_sessions_by_provider(&self, provider: &str) -> Vec<String> {
        self.session_providers
            .iter()
            .filter(|entry| entry.value() == provider)
            .map(|entry| entry.key().clone())
            .collect()
    }

    pub fn get_session_count_by_status(&self, status: meshag_connectors::SessionStatus) -> usize {
        self.sessions
            .iter()
            .filter(|entry| {
                std::mem::discriminant(&entry.value().status) == std::mem::discriminant(&status)
            })
            .count()
    }

    pub async fn read_websocket_message(
        &self,
        session_id: &str,
    ) -> Result<Option<tokio_tungstenite::tungstenite::Message>> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                match provider_name.as_str() {
                    "twilio" => {
                        if let Some(twilio_connector) = connector.as_any().downcast_ref::<meshag_connectors::providers::twilio::TwilioTransportConnector>() {
                            twilio_connector.read_message(session_id).await
                        } else {
                            Err(anyhow::anyhow!("Failed to cast connector to Twilio connector"))
                        }
                    }
                    _ => Err(anyhow::anyhow!("WebSocket reading not supported for provider: {}", provider_name))
                }
            } else {
                Err(anyhow::anyhow!(
                    "Connector not found for provider: {}",
                    provider_name
                ))
            }
        } else {
            Err(anyhow::anyhow!(
                "Provider not found for session: {}",
                session_id
            ))
        }
    }

    pub async fn write_websocket_message(
        &self,
        session_id: &str,
        message: tokio_tungstenite::tungstenite::Message,
    ) -> Result<()> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                match provider_name.as_str() {
                    "twilio" => {
                        if let Some(twilio_connector) = connector.as_any().downcast_ref::<meshag_connectors::providers::twilio::TwilioTransportConnector>() {
                            twilio_connector.write_message(session_id, message).await
                        } else {
                            Err(anyhow::anyhow!("Failed to cast connector to Twilio connector"))
                        }
                    }
                    _ => Err(anyhow::anyhow!("WebSocket writing not supported for provider: {}", provider_name))
                }
            } else {
                Err(anyhow::anyhow!(
                    "Connector not found for provider: {}",
                    provider_name
                ))
            }
        } else {
            Err(anyhow::anyhow!(
                "Provider not found for session: {}",
                session_id
            ))
        }
    }

    pub async fn publish_media_event(
        &self,
        session_id: &str,
        call_sid: &str,
        stream_sid: &str,
        track: &str,
        chunk: &str,
        timestamp: &str,
        payload: &str,
        media_format: TwilioMediaFormat,
    ) -> Result<String> {
        let media_payload = MediaEventPayload {
            call_sid: call_sid.to_string(),
            stream_sid: stream_sid.to_string(),
            track: track.to_string(),
            chunk: chunk.to_string(),
            timestamp: timestamp.to_string(),
            payload: payload.to_string(),
            media_format,
        };

        let event = ProcessingEvent {
            session_id: session_id.to_string(),
            conversation_id: Uuid::new_v4(), // Generate new conversation ID for each media event
            correlation_id: Uuid::new_v4(),
            event_type: "media_input".to_string(),
            payload: serde_json::to_value(media_payload)?,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let subject = SubjectName::STTSubject.as_str(Some(session_id.to_string()));
        let message_id = self.event_queue.publish_event(&subject, event).await?;

        Ok(message_id)
    }

    pub async fn publish_session_start_event(&self, session_id: &str) -> Result<String> {
        let event = ProcessingEvent {
            session_id: session_id.to_string(),
            conversation_id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            event_type: "session_start".to_string(),
            payload: serde_json::Value::Null,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let subject = SubjectName::STTSubject.as_str(Some(session_id.to_string()));
        let message_id = self.event_queue.publish_event(&subject, event).await?;

        Ok(message_id)
    }

    pub async fn publish_session_end_event(&self, session_id: &str) -> Result<String> {
        let event = ProcessingEvent {
            session_id: session_id.to_string(),
            conversation_id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            event_type: "session_end".to_string(),
            payload: serde_json::Value::Null,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let subject = format!("AUDIO_INPUT.session.{}", session_id);
        let message_id = self.event_queue.publish_event(&subject, event).await?;

        Ok(message_id)
    }
}

#[async_trait]
impl ServiceState for TransportServiceState {
    fn service_name(&self) -> String {
        "transport-service".to_string()
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateSessionRequest {
    pub provider: String,
    pub room_config: RoomConfig,
    pub participant_config: ParticipantConfig,
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub provider_counts: HashMap<String, usize>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    #[serde(rename = "session_created")]
    SessionCreated {
        session_id: String,
        room_url: String,
        meeting_token: Option<String>,
    },
    #[serde(rename = "session_updated")]
    SessionUpdated {
        session_id: String,
        status: String,
        participants: Vec<String>,
    },
    #[serde(rename = "session_ended")]
    SessionEnded { session_id: String, reason: String },
    #[serde(rename = "error")]
    Error {
        message: String,
        code: Option<String>,
    },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    // Twilio Stream events
    #[serde(rename = "connected")]
    TwilioConnected { protocol: String, version: String },
    #[serde(rename = "start")]
    TwilioStart {
        sequence_number: String,
        start: TwilioStartData,
    },
    #[serde(rename = "media")]
    TwilioMedia {
        sequence_number: String,
        media: TwilioMediaData,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseEvent {
    pub session_id: String,
    pub conversation_id: String,
    pub correlation_id: Uuid,
    pub event_type: String,
    pub payload: EventPayload,
    pub timestamp_ms: u64,
    pub source_service: String,
    pub target_service: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub audio_data: Vec<u8>,
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u8,
    pub duration_ms: u64,
    pub processing_time_ms: u64,
    pub provider: String,
}

pub struct WebSocketConnection {
    pub session_id: Option<String>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

impl WebSocketConnection {
    pub fn new() -> Self {
        Self {
            session_id: None,
            connected_at: chrono::Utc::now(),
        }
    }

    pub fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
            connected_at: chrono::Utc::now(),
        }
    }

    pub fn is_in_session(&self) -> bool {
        self.session_id.is_some()
    }

    pub fn get_session_id(&self) -> Option<&String> {
        self.session_id.as_ref()
    }

    pub fn join_session(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    pub fn leave_session(&mut self) {
        self.session_id = None;
    }

    pub fn connection_duration(&self) -> chrono::Duration {
        chrono::Utc::now() - self.connected_at
    }
}
