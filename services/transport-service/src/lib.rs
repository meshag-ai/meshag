use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use meshag_connectors::{
    Daily, DailyConfig, ParticipantConfig, RoomConfig, SessionInfo, TransportConnector,
    TransportRequest, TransportResponse,
};
use meshag_orchestrator::{AgentConfig, ConfigStorage};
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::EventQueue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// Transport service configuration
#[derive(Debug, Clone)]
pub struct TransportServiceConfig {
    pub daily_api_key: String,
    pub daily_domain: String,
    pub nats_url: String,
}

impl TransportServiceConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            daily_api_key: std::env::var("DAILY_API_KEY")
                .map_err(|_| anyhow::anyhow!("DAILY_API_KEY environment variable not set"))?,
            daily_domain: std::env::var("DAILY_DOMAIN")
                .map_err(|_| anyhow::anyhow!("DAILY_DOMAIN environment variable not set"))?,
            nats_url: std::env::var("NATS_URL")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
        })
    }
}

/// Transport service state
pub struct TransportServiceState {
    pub config: TransportServiceConfig,
    pub event_queue: Arc<EventQueue>,
    pub connectors: DashMap<String, Arc<dyn TransportConnector>>,
    pub sessions: DashMap<String, SessionInfo>,
    pub config_storage: Arc<ConfigStorage>,
    pub daily_domain: String,
}

impl TransportServiceState {
    pub async fn new(config: TransportServiceConfig) -> Result<Arc<Self>> {
        let event_queue = Arc::new(EventQueue::new(&config.nats_url).await?);
        let connectors = DashMap::new();
        let sessions = DashMap::new();
        let valkey_url =
            std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let config_storage = Arc::new(ConfigStorage::new(&valkey_url).await?);

        // Register Daily.co connector
        let daily_config =
            DailyConfig::new(config.daily_api_key.clone(), config.daily_domain.clone());
        let daily_connector = Daily::transport_connector(daily_config);
        connectors.insert("daily".to_string(), Arc::from(daily_connector));

        let daily_domain =
            std::env::var("DAILY_DOMAIN").unwrap_or_else(|_| "observeaia".to_string());

        Ok(Arc::new(Self {
            config,
            event_queue,
            connectors,
            sessions,
            config_storage,
            daily_domain,
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

        self.sessions.insert(session_id, session_info);

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
        self.sessions.remove(session_id);
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

                // Create meeting token for the agent
                let agent_token = if let Some(daily_connector) = connector.as_any().downcast_ref::<meshag_connectors::providers::daily::DailyTransportConnector>() {
                    daily_connector.create_meeting_token(session_id, &agent_participant_config).await?
                } else {
                    return Err(anyhow::anyhow!("Failed to cast connector to Daily.co connector"));
                };

                // Start the AI agent processing pipeline
                self.start_ai_agent_pipeline(session_id, &agent_token)
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
    async fn start_ai_agent_pipeline(&self, session_id: &str, agent_token: &str) -> Result<()> {
        // Load configuration for this session
        let _config = self
            .get_config(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No configuration found for session: {}", session_id))?;

        // Create orchestrator router for this session
        let mut router = meshag_orchestrator::ServiceRouter::new(
            &self.config.nats_url,
            (*self.event_queue).clone(),
            "transport".to_string(),
        )
        .await?;

        // Load the configuration
        router.load_config(session_id).await?;

        // Start background tasks for the AI agent
        let event_queue = self.event_queue.clone();
        let session_id_clone = session_id.to_string();
        let agent_token_clone = agent_token.to_string();

        // Task 1: Listen for audio from Daily.co and send to STT
        let event_queue1 = event_queue.clone();
        let session_id1 = session_id_clone.clone();
        let agent_token1 = agent_token_clone.clone();
        let router1 = router.clone();
        let state1 = self.clone();
        tokio::spawn(async move {
            if let Err(e) = state1
                .handle_daily_audio_input(&session_id1, &agent_token1, &event_queue1, &router1)
                .await
            {
                tracing::error!("Error handling Daily.co audio input: {}", e);
            }
        });

        // Task 2: Listen for processed audio from TTS and send to Daily.co
        let event_queue2 = event_queue.clone();
        let session_id2 = session_id_clone.clone();
        let agent_token2 = agent_token_clone.clone();
        let router2 = router.clone();
        let state2 = self.clone();
        tokio::spawn(async move {
            if let Err(e) = state2
                .handle_tts_audio_output(&session_id2, &agent_token2, &event_queue2, &router2)
                .await
            {
                tracing::error!("Error handling TTS audio output: {}", e);
            }
        });

        // Send initial greeting
        self.send_ai_greeting(&session_id_clone, &agent_token_clone, &router)
            .await?;

        Ok(())
    }

    /// Handle audio input from Daily.co and send to STT service
    async fn handle_daily_audio_input(
        &self,
        session_id: &str,
        agent_token: &str,
        event_queue: &meshag_shared::EventQueue,
        router: &meshag_orchestrator::ServiceRouter,
    ) -> Result<()> {
        tracing::info!(
            "AI agent listening for audio from Daily.co room: {}",
            session_id
        );

        // Connect to Daily.co WebSocket using agent token
        let ws_url = format!("wss://{}.daily.co/{}", self.daily_domain, session_id);
        let mut ws_stream = tokio_tungstenite::connect_async(&ws_url).await?;

        tracing::info!(
            "Connected to Daily.co WebSocket for session: {}",
            session_id
        );

        // Listen for audio events from users
        while let Some(msg) = ws_stream.0.next().await {
            match msg {
                Ok(Message::Binary(audio_data)) => {
                    // Create minimal event payload
                    let payload = serde_json::json!({
                        "audio_data": general_purpose::STANDARD.encode(&audio_data),
                        "format": "pcm",
                        "sample_rate": 16000,
                        "channels": 1
                    });

                    // Route using config-based pipeline
                    if let Err(e) = router.route_to_next(payload, session_id).await {
                        tracing::error!("Failed to route audio: {}", e);
                    }
                }
                Ok(Message::Text(text)) => {
                    // Handle text messages (e.g., participant events)
                    tracing::debug!("Received text message: {}", text);
                }
                Ok(Message::Close(_)) => {
                    tracing::info!(
                        "Daily.co WebSocket connection closed for session: {}",
                        session_id
                    );
                    break;
                }
                Err(e) => {
                    tracing::error!("WebSocket error for session {}: {}", session_id, e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Handle processed audio from TTS service and send to Daily.co
    async fn handle_tts_audio_output(
        &self,
        session_id: &str,
        agent_token: &str,
        event_queue: &meshag_shared::EventQueue,
        router: &meshag_orchestrator::ServiceRouter,
    ) -> Result<()> {
        tracing::info!(
            "AI agent listening for TTS output for session: {}",
            session_id
        );

        // Listen to NATS stream for TTS output
        let stream_config = meshag_shared::StreamConfig {
            name: "transport.audio".to_string(),
            subjects: vec!["transport.audio".to_string()],
            max_messages: 10000,
            max_bytes: 1024 * 1024 * 100, // 100MB
            max_age: std::time::Duration::from_secs(3600),
        };

        let mut consumer = event_queue
            .create_consumer(stream_config, "transport.audio".to_string())
            .await?;

        // Use batch method to consume messages
        loop {
            match consumer.batch().max_messages(10).await {
                Ok(messages) => {
                    for message in messages {
                        if let Ok(payload) = serde_json::from_slice::<meshag_shared::ProcessingEvent>(
                            &message.payload,
                        ) {
                            // Check if this event is for our session
                            if payload.conversation_id.to_string() == session_id {
                                tracing::debug!("Received TTS audio for session: {}", session_id);

                                // Extract audio data from the event
                                if let Some(audio_data_b64) = payload.payload.get("audio_data") {
                                    if let Some(audio_data_str) = audio_data_b64.as_str() {
                                        if let Ok(audio_data) =
                                            general_purpose::STANDARD.decode(audio_data_str)
                                        {
                                            // Send audio data to Daily.co room using agent token
                                            if let Err(e) = self
                                                .send_audio_to_daily(
                                                    session_id,
                                                    agent_token,
                                                    &audio_data,
                                                )
                                                .await
                                            {
                                                tracing::error!(
                                                    "Failed to send audio to Daily.co: {}",
                                                    e
                                                );
                                            } else {
                                                tracing::debug!("Sent audio data to Daily.co room");
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Acknowledge the message
                        if let Err(e) = message.ack().await {
                            tracing::error!("Failed to acknowledge message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving TTS audio event: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Send audio data to Daily.co room
    async fn send_audio_to_daily(
        &self,
        session_id: &str,
        agent_token: &str,
        audio_data: &[u8],
    ) -> Result<()> {
        // Connect to Daily.co WebSocket to send audio
        let ws_url = format!("wss://{}.daily.co/{}", self.daily_domain, session_id);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await?;

        // Send audio data as binary message
        ws_stream.send(Message::Binary(audio_data.to_vec())).await?;

        tracing::debug!(
            "Sent {} bytes of audio to Daily.co room {}",
            audio_data.len(),
            session_id
        );
        Ok(())
    }

    /// Send AI greeting when user joins
    async fn send_ai_greeting(
        &self,
        session_id: &str,
        agent_token: &str,
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

    fn get_provider_for_session(&self, _session_id: &str) -> Option<String> {
        // In a real implementation, you'd store the provider mapping
        // For now, assume Daily.co
        Some("daily".to_string())
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

        // Add queue metrics
        if let Ok(queue_metrics) = self.event_queue.get_metrics("transport.input").await {
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
}
