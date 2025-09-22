use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use meshag_connectors::{
    Daily, DailyConfig, ParticipantConfig, RoomConfig, SessionInfo, TransportConnector,
    TransportRequest, TransportResponse,
};
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::EventQueue;
use std::collections::HashMap;
use std::sync::Arc;
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
}

impl TransportServiceState {
    pub async fn new(config: TransportServiceConfig) -> Result<Arc<Self>> {
        let event_queue = Arc::new(EventQueue::new(&config.nats_url).await?);
        let connectors = DashMap::new();
        let sessions = DashMap::new();

        // Register Daily.co connector
        let daily_config =
            DailyConfig::new(config.daily_api_key.clone(), config.daily_domain.clone());
        let daily_connector = Daily::transport_connector(daily_config);
        connectors.insert("daily".to_string(), Arc::from(daily_connector));

        Ok(Arc::new(Self {
            config,
            event_queue,
            connectors,
            sessions,
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
