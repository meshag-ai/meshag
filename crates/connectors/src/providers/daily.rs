use crate::transport::{
    ParticipantConfig, ParticipantInfo, ParticipantPermissions, RoomConfig, RoomPrivacy,
    SessionInfo, SessionStatus, TransportConnector, TransportRequest, TransportResponse,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

/// Daily.co provider configuration
#[derive(Debug, Clone)]
pub struct DailyConfig {
    pub api_key: String,
    pub domain: String,
    pub base_url: String,
}

impl DailyConfig {
    pub fn new(api_key: String, domain: String) -> Self {
        Self {
            api_key,
            domain: domain.clone(),
            base_url: format!("https://{}.daily.co", domain),
        }
    }

    pub fn with_custom_domain(mut self, custom_domain: String) -> Self {
        self.base_url = format!("https://{}", custom_domain);
        self
    }
}

/// Daily.co Transport Connector
#[derive(Debug, Clone)]
pub struct DailyTransportConnector {
    config: DailyConfig,
    client: reqwest::Client,
}

impl DailyTransportConnector {
    pub fn new(config: DailyConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn generate_room_name(&self) -> String {
        format!("room-{}", Uuid::new_v4().to_string()[..8].to_lowercase())
    }

    async fn create_room(&self, room_config: &RoomConfig) -> Result<serde_json::Value> {
        let room_name = room_config
            .name
            .clone()
            .unwrap_or_else(|| self.generate_room_name());

        let privacy = match room_config.privacy {
            RoomPrivacy::Public => "public",
            RoomPrivacy::Private => "private",
        };

        let mut properties = json!({
            "start_video_off": true,
            "start_audio_off": false,
            "enable_chat": room_config.enable_chat,
            "enable_knocking": privacy == "private",
            "enable_screenshare": true,
            "enable_recording": if room_config.enable_recording { "cloud" } else { "off" },
            "enable_transcription": room_config.enable_transcription,
        });

        if let Some(max_participants) = room_config.max_participants {
            properties["max_participants"] = json!(max_participants);
        }

        // Add inactivity timeout settings
        if let Some(eject_after_elapsed) = room_config.eject_after_elapsed {
            properties["eject_after_elapsed"] = json!(eject_after_elapsed);
        }

        if let Some(eject_at_room_exp) = room_config.eject_at_room_exp {
            properties["eject_at_room_exp"] = json!(eject_at_room_exp);
        }

        let request_body = json!({
            "name": room_name,
            "privacy": privacy,
            "properties": properties
        });

        let response = self
            .client
            .post("https://api.daily.co/v1/rooms")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Daily.co API error: {}", error_text));
        }

        let room_data: serde_json::Value = response.json().await?;
        Ok(room_data)
    }

    async fn get_room_info(&self, room_name: &str) -> Result<serde_json::Value> {
        let response = self
            .client
            .get(&format!("https://api.daily.co/v1/rooms/{}", room_name))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Daily.co room info error: {}", error_text));
        }

        let room_data: serde_json::Value = response.json().await?;
        Ok(room_data)
    }

    async fn get_room_presence(&self, room_name: &str) -> Result<Vec<ParticipantInfo>> {
        let response = self
            .client
            .get(&format!(
                "https://api.daily.co/v1/rooms/{}/presence",
                room_name
            ))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            // Room might be empty or not active
            return Ok(vec![]);
        }

        let presence_data: serde_json::Value = response.json().await?;
        let mut participants = vec![];

        if let Some(presence_list) = presence_data["presence"].as_array() {
            for participant in presence_list {
                if let Some(participant_id) = participant["id"].as_str() {
                    participants.push(ParticipantInfo {
                        id: participant_id.to_string(),
                        name: participant["user_name"].as_str().map(|s| s.to_string()),
                        joined_at: chrono::Utc::now(), // Daily doesn't provide exact join time in presence
                        is_owner: participant["is_owner"].as_bool().unwrap_or(false),
                        permissions: ParticipantPermissions {
                            can_admin: participant["is_owner"].as_bool().unwrap_or(false),
                            can_send_video: !participant["start_video_off"]
                                .as_bool()
                                .unwrap_or(false),
                            can_send_audio: !participant["start_audio_off"]
                                .as_bool()
                                .unwrap_or(false),
                            can_send_screen_video: participant["enable_screenshare"]
                                .as_bool()
                                .unwrap_or(true),
                            can_send_screen_audio: participant["enable_screenshare"]
                                .as_bool()
                                .unwrap_or(true),
                        },
                    });
                }
            }
        }

        Ok(participants)
    }
}

#[async_trait]
impl TransportConnector for DailyTransportConnector {
    fn provider_name(&self) -> &'static str {
        "daily"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self
            .client
            .get("https://api.daily.co/v1/")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    async fn create_session(&self, request: TransportRequest) -> Result<TransportResponse> {
        // Create the room first
        let room_data = self.create_room(&request.room_config).await?;

        let room_name = room_data["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid room name from Daily.co"))?;

        let room_url = room_data["url"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid room URL from Daily.co"))?;

        // Create meeting token for the participant
        let meeting_token = self
            .create_meeting_token(room_name, request.participant_config.clone())
            .await?;

        let mut provider_metadata = HashMap::new();
        provider_metadata.insert("daily_room_data".to_string(), room_data.clone());

        Ok(TransportResponse {
            session_id: request.session_id,
            room_url: room_url.to_string(),
            room_name: room_name.to_string(),
            meeting_token: Some(meeting_token),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            provider_metadata,
        })
    }

    async fn get_session(&self, session_id: &str) -> Result<SessionInfo> {
        // For Daily.co, we need to map session_id to room_name
        // In a real implementation, you'd store this mapping in a database
        // For now, we'll assume session_id is the room_name
        let room_name = session_id;

        let room_data = self.get_room_info(room_name).await?;
        let participants = self.get_room_presence(room_name).await?;

        let status = if participants.is_empty() {
            SessionStatus::Created
        } else {
            SessionStatus::Active
        };

        let created_at = room_data["created_at"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Ok(SessionInfo {
            session_id: session_id.to_string(),
            room_name: room_name.to_string(),
            room_url: room_data["url"].as_str().unwrap_or("").to_string(),
            status,
            participants,
            created_at,
            updated_at: chrono::Utc::now(),
        })
    }

    async fn end_session(&self, session_id: &str) -> Result<()> {
        // For Daily.co, ending a session means deleting the room
        let room_name = session_id;

        let response = self
            .client
            .delete(&format!("https://api.daily.co/v1/rooms/{}", room_name))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Daily.co delete room error: {}", error_text));
        }

        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let response = self
            .client
            .get("https://api.daily.co/v1/rooms")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Daily.co list rooms error: {}", error_text));
        }

        let rooms_data: serde_json::Value = response.json().await?;
        let mut sessions = vec![];

        if let Some(rooms) = rooms_data["data"].as_array() {
            for room in rooms {
                if let Some(room_name) = room["name"].as_str() {
                    // Get session info for each room
                    if let Ok(session_info) = self.get_session(room_name).await {
                        sessions.push(session_info);
                    }
                }
            }
        }

        Ok(sessions)
    }

    async fn create_meeting_token(
        &self,
        session_id: &str,
        participant: ParticipantConfig,
    ) -> Result<String> {
        let mut properties = json!({
            "room_name": session_id,
            "is_owner": participant.is_owner,
            "user_name": participant.name,
            "enable_screenshare": participant.permissions.can_send_screen_video,
            "start_video_off": !participant.permissions.can_send_video,
            "start_audio_off": !participant.permissions.can_send_audio,
        });

        // Set expiration to 1 hour from now
        let exp = chrono::Utc::now().timestamp() + 3600;
        properties["exp"] = json!(exp);

        let request_body = json!({
            "properties": properties
        });

        let response = self
            .client
            .post("https://api.daily.co/v1/meeting-tokens")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Daily.co meeting token error: {}", error_text));
        }

        let token_data: serde_json::Value = response.json().await?;
        let token = token_data["token"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid token response from Daily.co"))?
            .to_string();

        Ok(token)
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "Daily.co API key"
                },
                "domain": {
                    "type": "string",
                    "description": "Daily.co domain name"
                },
                "custom_domain": {
                    "type": "string",
                    "description": "Custom domain (optional)"
                }
            },
            "required": ["api_key", "domain"]
        })
    }
}

/// Factory functions for Daily.co connectors
pub struct Daily;

impl Daily {
    /// Create Daily.co transport connector
    pub fn transport_connector(config: DailyConfig) -> Box<dyn TransportConnector> {
        Box::new(DailyTransportConnector::new(config))
    }
}
