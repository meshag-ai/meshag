use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use meshag_connectors::{
    twilio_transport_connector, ParticipantConfig, RoomConfig, SessionInfo, TransportConnector,
    TransportRequest, TransportResponse, TwilioConfig,
};
use meshag_orchestrator::{AgentConfig, ConfigStorage};
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::{
    EventQueue, MediaEventPayload, ProcessingEvent, StreamConfig, TwilioMediaData,
    TwilioMediaFormat, TwilioStartData,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Transport service configuration
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

/// Transport service state
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

        // Register Twilio connector if credentials are provided
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

        // Store session info
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

        // Store the provider mapping for this session
        self.session_providers
            .insert(session_id, request.provider.clone());

        Ok(response)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<SessionInfo> {
        if let Some(session) = self.sessions.get(session_id) {
            // Update session info from provider
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

        // Clean up session data
        self.sessions.remove(session_id);
        self.session_providers.remove(session_id);

        // Also clean up any stored configuration for this session
        if let Err(e) = self.delete_config(session_id).await {
            tracing::warn!("Failed to delete config for session {}: {}", session_id, e);
        }

        Ok(())
    }

    /// Join as AI agent and start processing pipeline
    pub async fn join_as_ai_agent(&self, session_id: &str, agent_name: &str) -> Result<()> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                // Create participant config for the agent
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

                // Create meeting token for the agent using the connector
                let agent_token = connector
                    .create_meeting_token(session_id, agent_participant_config.clone())
                    .await?;

                // Start the AI agent processing pipeline
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

    /// Start the AI agent processing pipeline
    async fn start_ai_agent_pipeline(
        &self,
        session_id: &str,
        agent_token: &str,
        provider_name: &str,
    ) -> Result<()> {
        // Load configuration for this session
        let _config = self
            .get_config(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No configuration found for session: {}", session_id))?;

        // Create orchestrator router for this session
        let mut router = meshag_orchestrator::ServiceRouter::new(
            &self.config.valkey_url,
            (*self.event_queue).clone(),
            "transport".to_string(),
        )
        .await?;

        // Load the configuration
        router.load_config(session_id).await?;

        // Start background tasks for the AI agent based on provider

        match provider_name {
            "daily" => {
                // For Daily.co, the connector will handle WebSocket connections internally
                // The transport service no longer manages Daily.co specific WebSocket handling
                tracing::info!(
                    "Daily.co provider detected - connector handles WebSocket internally"
                );
            }
            "twilio" => {
                // For Twilio, we don't need WebSocket handling as it uses phone calls
                // The audio processing will be handled through webhooks
                tracing::info!("Twilio provider detected - audio processing via webhooks");
            }
            _ => {
                tracing::warn!(
                    "Unknown provider: {} - no specific audio handling implemented",
                    provider_name
                );
            }
        }

        // Send initial greeting
        self.send_ai_greeting(session_id, agent_token, &router)
            .await?;

        Ok(())
    }

    /// Send AI greeting when user joins
    async fn send_ai_greeting(
        &self,
        session_id: &str,
        _agent_token: &str,
        router: &meshag_orchestrator::ServiceRouter,
    ) -> Result<()> {
        tracing::info!("AI agent sending greeting for session: {}", session_id);

        // Generate greeting text
        let greeting_text = "Hello! I'm your AI assistant. How can I help you today?";

        // Route using config-based pipeline
        if let Err(e) = router
            .route_to_next(serde_json::json!({"text": greeting_text}), session_id)
            .await
        {
            tracing::error!("Failed to send greeting: {}", e);
        }

        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        // For now, just return sessions from our local cache
        // In a real implementation, you'd also query the providers
        let sessions: Vec<SessionInfo> = self
            .sessions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        Ok(sessions)
    }

    fn get_provider_for_session(&self, session_id: &str) -> Option<String> {
        // Look up the provider for this session
        if let Some(provider) = self.session_providers.get(session_id) {
            Some(provider.clone())
        } else {
            // If session not found in cache, return None
            // This handles cases where sessions might have been created before
            // the provider mapping was implemented
            None
        }
    }

    /// Store configuration for a session
    pub async fn store_config(&self, session_id: &str, config: &AgentConfig) -> Result<()> {
        self.config_storage.store_config(session_id, config).await
    }

    /// Retrieve configuration for a session
    pub async fn get_config(&self, session_id: &str) -> Result<Option<AgentConfig>> {
        self.config_storage.get_config(session_id).await
    }

    /// Delete configuration for a session
    pub async fn delete_config(&self, session_id: &str) -> Result<()> {
        self.config_storage.delete_config(session_id).await
    }

    /// List all stored configurations
    pub async fn list_configs(&self) -> Result<Vec<String>> {
        self.config_storage.list_configs().await
    }

    /// Check if configuration exists for a session
    pub async fn config_exists(&self, session_id: &str) -> Result<bool> {
        self.config_storage.config_exists(session_id).await
    }

    /// Get session statistics
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

    /// Check if a session exists
    pub fn session_exists(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Get all session IDs
    pub fn get_session_ids(&self) -> Vec<String> {
        self.sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get sessions by provider
    pub fn get_sessions_by_provider(&self, provider: &str) -> Vec<String> {
        self.session_providers
            .iter()
            .filter(|entry| entry.value() == provider)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get session count by status
    pub fn get_session_count_by_status(&self, status: meshag_connectors::SessionStatus) -> usize {
        self.sessions
            .iter()
            .filter(|entry| {
                std::mem::discriminant(&entry.value().status) == std::mem::discriminant(&status)
            })
            .count()
    }

    /// Check if service is healthy (all dependencies available)
    pub async fn is_service_healthy(&self) -> bool {
        let health_checks = self.is_ready().await;
        health_checks.iter().all(|check| check.status == "healthy")
    }

    /// Read message from WebSocket connection via connector
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

    /// Write message to WebSocket connection via connector
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

    /// Publish media event to STT service via NATS
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
            session_id: session_id.to_string(),
            call_sid: call_sid.to_string(),
            stream_sid: stream_sid.to_string(),
            track: track.to_string(),
            chunk: chunk.to_string(),
            timestamp: timestamp.to_string(),
            payload: payload.to_string(),
            media_format,
        };

        let event = ProcessingEvent {
            conversation_id: Uuid::new_v4(), // Generate new conversation ID for each media event
            correlation_id: Uuid::new_v4(),
            event_type: "media_input".to_string(),
            payload: serde_json::to_value(media_payload)?,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            source_service: "transport-service".to_string(),
            target_service: "stt-service".to_string(),
        };

        let subject = "AUDIO_INPUT";
        let message_id = self.event_queue.publish_event(subject, event).await?;

        // tracing::info!(
        //     session_id = %session_id,
        //     call_sid = %call_sid,
        //     track = %track,
        //     chunk = %chunk,
        //     message_id = %message_id,
        //     "Published media event to STT service"
        // );

        Ok(message_id)
    }

    /// Start consuming response events and writing them to WebSocket
    pub async fn start_response_consumer(&self) -> Result<()> {
        let event_queue = self.event_queue.clone();
        let session_providers = self.session_providers.clone();
        let connectors = self.connectors.clone();

        tokio::spawn(async move {
            let stream_config = StreamConfig::tts_output();
            let subject = "TTS_OUTPUT".to_string();

            if let Err(e) = event_queue.consume_events(stream_config, subject, move |event| {
                let session_providers = session_providers.clone();
                let connectors = connectors.clone();

                async move {
                    if let Some(payload) = event.payload.get("session_id") {
                        if let Some(session_id) = payload.as_str() {
                            // Parse response payload
                            if let Ok(response_payload) = serde_json::from_value::<ResponseEventPayload>(event.payload.clone()) {
                                tracing::info!(
                                    session_id = %session_id,
                                    response_type = %response_payload.response_type,
                                    "Received response event for WebSocket"
                                );

                                // Write response to WebSocket
                                if let Some(provider_entry) = session_providers.get(session_id) {
                                    let provider_name = provider_entry.value().clone();
                                    if let Some(connector) = connectors.get(&provider_name) {
                                        match provider_name.as_str() {
                                            "twilio" => {
                                                if let Some(twilio_connector) = connector.as_any().downcast_ref::<meshag_connectors::providers::twilio::TwilioTransportConnector>() {
                                                    // Create Twilio media message for outbound audio
                                                    let media_message = serde_json::json!({
                                                        "event": "media",
                                                        "streamSid": response_payload.call_sid,
                                                        "media": {
                                                            "track": "outbound",
                                                            "chunk": "1",
                                                            "timestamp": chrono::Utc::now().timestamp_millis().to_string(),
                                                            "payload": response_payload.data
                                                        }
                                                    });

                                                    let ws_message = tokio_tungstenite::tungstenite::Message::Text(
                                                        serde_json::to_string(&media_message)?
                                                    );

                                                    if let Err(e) = twilio_connector.write_message(session_id, ws_message).await {
                                                        tracing::error!(
                                                            session_id = %session_id,
                                                            error = %e,
                                                            "Failed to write response to WebSocket"
                                                        );
                                                    }
                                                }
                                            }
                                            _ => {
                                                tracing::warn!(
                                                    session_id = %session_id,
                                                    provider = %provider_name,
                                                    "Response writing not supported for provider"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(())
                }
            }).await {
                tracing::error!("Response consumer error: {}", e);
            }
        });

        Ok(())
    }

    /// Check if WebSocket is connected via connector
    pub async fn is_websocket_connected(&self, session_id: &str) -> Result<bool> {
        if let Some(provider_name) = self.get_provider_for_session(session_id) {
            if let Some(connector) = self.get_connector(&provider_name) {
                match provider_name.as_str() {
                    "twilio" => {
                        if let Some(twilio_connector) = connector.as_any().downcast_ref::<meshag_connectors::providers::twilio::TwilioTransportConnector>() {
                            Ok(twilio_connector.is_websocket_connected(session_id).await)
                        } else {
                            Err(anyhow::anyhow!("Failed to cast connector to Twilio connector"))
                        }
                    }
                    _ => Err(anyhow::anyhow!("WebSocket connection check not supported for provider: {}", provider_name))
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
}

#[async_trait]
impl ServiceState for TransportServiceState {
    fn service_name(&self) -> String {
        "transport-service".to_string()
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
        let connector_names: Vec<String> = self
            .connectors
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for connector_name in connector_names {
            if let Some(connector) = self.connectors.get(&connector_name) {
                let healthy = connector.health_check().await.unwrap_or(false);
                checks.push(HealthCheck {
                    name: format!("connector_{}", connector_name),
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
        }

        checks
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }

    async fn get_metrics(&self) -> Vec<String> {
        let mut metrics = vec![];

        metrics.push(format!("active_sessions {}", self.sessions.len()));
        metrics.push(format!("registered_connectors {}", self.connectors.len()));
        metrics.push(format!(
            "session_provider_mappings {}",
            self.session_providers.len()
        ));

        // Add provider-specific session counts
        let mut provider_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for provider in self.session_providers.iter() {
            *provider_counts.entry(provider.value().clone()).or_insert(0) += 1;
        }
        for (provider, count) in provider_counts {
            metrics.push(format!("sessions_by_provider_{}  {}", provider, count));
        }

        // Add queue metrics
        if let Ok(queue_metrics) = self.event_queue.get_metrics("transport-input").await {
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

/// Request to create a new session
#[derive(Debug, serde::Deserialize)]
pub struct CreateSessionRequest {
    pub provider: String,
    pub room_config: RoomConfig,
    pub participant_config: ParticipantConfig,
    pub options: HashMap<String, serde_json::Value>,
}

/// Session statistics
#[derive(Debug, serde::Serialize)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub provider_counts: HashMap<String, usize>,
}

/// WebSocket message types
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

/// Response event payload for WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseEventPayload {
    pub session_id: String,
    pub call_sid: String,
    pub response_type: String, // "tts_audio", "transcription", etc.
    pub data: String,          // Base64 encoded response data
    pub metadata: HashMap<String, String>,
}

/// WebSocket connection state
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
