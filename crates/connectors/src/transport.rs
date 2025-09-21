use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportRequest {
    pub session_id: String,
    pub room_config: RoomConfig,
    pub participant_config: ParticipantConfig,
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportResponse {
    pub session_id: String,
    pub room_url: String,
    pub room_name: String,
    pub meeting_token: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    pub name: Option<String>,
    pub privacy: RoomPrivacy,
    pub max_participants: Option<u32>,
    pub enable_recording: bool,
    pub enable_transcription: bool,
    pub enable_chat: bool,
    pub auto_start_recording: bool,
    pub auto_start_transcription: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomPrivacy {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantConfig {
    pub name: Option<String>,
    pub is_owner: bool,
    pub permissions: ParticipantPermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantPermissions {
    pub can_admin: bool,
    pub can_send_video: bool,
    pub can_send_audio: bool,
    pub can_send_screen_video: bool,
    pub can_send_screen_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub room_name: String,
    pub room_url: String,
    pub status: SessionStatus,
    pub participants: Vec<ParticipantInfo>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Created,
    Active,
    Ended,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub id: String,
    pub name: Option<String>,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub is_owner: bool,
    pub permissions: ParticipantPermissions,
}

/// Generic transport connector trait that all providers must implement
#[async_trait]
pub trait TransportConnector: Send + Sync {
    /// Unique identifier for this connector
    fn provider_name(&self) -> &'static str;

    /// Check if the connector is healthy and ready to process requests
    async fn health_check(&self) -> Result<bool>;

    /// Create a new session/room
    async fn create_session(&self, request: TransportRequest) -> Result<TransportResponse>;

    /// Get session information
    async fn get_session(&self, session_id: &str) -> Result<SessionInfo>;

    /// End a session
    async fn end_session(&self, session_id: &str) -> Result<()>;

    /// List active sessions
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Get provider-specific configuration schema
    fn config_schema(&self) -> serde_json::Value;
}
