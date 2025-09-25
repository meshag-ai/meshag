use crate::transport::{
    ParticipantConfig, SessionInfo, SessionStatus, TransportConnector, TransportRequest,
    TransportResponse,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use uuid::Uuid;

/// Twilio provider configuration
#[derive(Debug, Clone)]
pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub phone_number: String,
    pub webhook_url: String,
    pub base_url: String,
}

impl TwilioConfig {
    pub fn new(account_sid: String, auth_token: String, phone_number: String) -> Self {
        Self {
            account_sid,
            auth_token,
            phone_number,
            webhook_url: "".to_string(),
            base_url: "https://api.twilio.com".to_string(),
        }
    }

    pub fn with_webhook_url(mut self, webhook_url: String) -> Self {
        self.webhook_url = webhook_url;
        self
    }

    pub fn with_custom_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

/// Twilio Transport Connector
#[derive(Debug, Clone)]
pub struct TwilioTransportConnector {
    config: TwilioConfig,
    client: reqwest::Client,
    websocket_connections: Arc<
        RwLock<
            HashMap<
                String,
                Arc<
                    RwLock<
                        Option<
                            WebSocketStream<
                                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                            >,
                        >,
                    >,
                >,
            >,
        >,
    >,
}

impl TwilioTransportConnector {
    pub fn new(config: TwilioConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            websocket_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_call_sid(&self) -> String {
        format!("CA{}", Uuid::new_v4().to_string().replace('-', ""))
    }

    async fn make_twilio_request(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/2010-04-01/Accounts/{}/{}",
            self.config.base_url, self.config.account_sid, endpoint
        );

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.account_sid, Some(&self.config.auth_token))
            .form(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            let response_text = response.text().await?;
            serde_json::from_str(&response_text)
                .map_err(|e| anyhow!("Failed to parse Twilio response: {}", e))
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("Twilio API error: {}", error_text))
        }
    }

    /// Connect to Twilio WebSocket for a session
    async fn connect_websocket(&self, session_id: &str) -> Result<()> {
        // For Twilio, we'll simulate a WebSocket connection
        // In a real implementation, this would connect to Twilio's WebSocket API
        let ws_url = format!("wss://stream.twilio.com/v1/Streams/{}", session_id);

        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                let mut connections = self.websocket_connections.write().await;
                connections.insert(
                    session_id.to_string(),
                    Arc::new(RwLock::new(Some(ws_stream))),
                );
                tracing::info!("Connected to Twilio WebSocket for session: {}", session_id);
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to connect to Twilio WebSocket for session {}: {}",
                    session_id,
                    e
                );
                // For now, we'll create a placeholder connection
                let mut connections = self.websocket_connections.write().await;
                connections.insert(session_id.to_string(), Arc::new(RwLock::new(None)));
                Ok(())
            }
        }
    }

    /// Disconnect WebSocket for a session
    async fn disconnect_websocket(&self, session_id: &str) -> Result<()> {
        let mut connections = self.websocket_connections.write().await;
        if connections.remove(session_id).is_some() {
            tracing::info!("Disconnected Twilio WebSocket for session: {}", session_id);
        }
        Ok(())
    }

    /// Read from WebSocket connection
    async fn read_websocket(&self, session_id: &str) -> Result<Option<Message>> {
        let connections = self.websocket_connections.read().await;
        if let Some(connection) = connections.get(session_id) {
            let mut connection_guard = connection.write().await;
            if let Some(ws_stream) = connection_guard.as_mut() {
                use futures_util::StreamExt;
                match ws_stream.next().await {
                    Some(Ok(message)) => Ok(Some(message)),
                    Some(Err(e)) => Err(anyhow!("WebSocket read error: {}", e)),
                    None => Ok(None),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(anyhow!(
                "No WebSocket connection found for session: {}",
                session_id
            ))
        }
    }

    /// Write to WebSocket connection
    async fn write_websocket(&self, session_id: &str, message: Message) -> Result<()> {
        let connections = self.websocket_connections.read().await;
        if let Some(connection) = connections.get(session_id) {
            let mut connection_guard = connection.write().await;
            if let Some(ws_stream) = connection_guard.as_mut() {
                use futures_util::SinkExt;
                ws_stream.send(message).await?;
                Ok(())
            } else {
                Err(anyhow!(
                    "No active WebSocket connection for session: {}",
                    session_id
                ))
            }
        } else {
            Err(anyhow!(
                "No WebSocket connection found for session: {}",
                session_id
            ))
        }
    }

    /// Public method to read from WebSocket connection
    pub async fn read_message(&self, session_id: &str) -> Result<Option<Message>> {
        self.read_websocket(session_id).await
    }

    /// Public method to write to WebSocket connection
    pub async fn write_message(&self, session_id: &str, message: Message) -> Result<()> {
        self.write_websocket(session_id, message).await
    }

    /// Public method to check if WebSocket is connected
    pub async fn is_websocket_connected(&self, session_id: &str) -> bool {
        let connections = self.websocket_connections.read().await;
        if let Some(connection) = connections.get(session_id) {
            connection.read().await.is_some()
        } else {
            false
        }
    }
}

#[async_trait]
impl TransportConnector for TwilioTransportConnector {
    fn provider_name(&self) -> &'static str {
        "twilio"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn health_check(&self) -> Result<bool> {
        // Test Twilio API connectivity
        let response = self.make_twilio_request("Account.json", json!({})).await;
        Ok(response.is_ok())
    }

    async fn create_session(&self, request: TransportRequest) -> Result<TransportResponse> {
        let session_id = Uuid::new_v4().to_string();
        let call_sid = self.generate_call_sid();

        // Create Twilio call
        let call_payload = json!({
            "To": request.participant_config.name.as_ref().unwrap_or(&"".to_string()),
            "From": self.config.phone_number,
            "Url": self.config.webhook_url,
            "StatusCallback": format!("{}/status", self.config.webhook_url),
            "StatusCallbackEvent": ["initiated", "ringing", "answered", "completed"],
            "Record": true,
            "RecordingStatusCallback": format!("{}/recording", self.config.webhook_url)
        });

        let _response = self.make_twilio_request("Calls.json", call_payload).await?;

        // Connect WebSocket for this session
        self.connect_websocket(&session_id).await?;

        Ok(TransportResponse {
            session_id,
            room_url: format!("twilio://call/{}", call_sid),
            room_name: call_sid.clone(),
            meeting_token: Some(call_sid),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            provider_metadata: HashMap::new(),
        })
    }

    async fn get_session(&self, session_id: &str) -> Result<SessionInfo> {
        // Get call details from Twilio
        let response = self
            .make_twilio_request(&format!("Calls/{}.json", session_id), json!({}))
            .await?;

        let status = match response["status"].as_str() {
            Some("queued") | Some("ringing") => SessionStatus::Created,
            Some("in-progress") => SessionStatus::Active,
            Some("completed") | Some("busy") | Some("no-answer") | Some("canceled")
            | Some("failed") => SessionStatus::Ended,
            _ => SessionStatus::Created,
        };

        Ok(SessionInfo {
            session_id: session_id.to_string(),
            room_name: session_id.to_string(),
            room_url: format!("twilio://call/{}", session_id),
            status,
            participants: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn end_session(&self, session_id: &str) -> Result<()> {
        // Disconnect WebSocket first
        self.disconnect_websocket(session_id).await?;

        // Hang up the Twilio call
        let hangup_payload = json!({
            "Status": "completed"
        });

        self.make_twilio_request(&format!("Calls/{}.json", session_id), hangup_payload)
            .await?;

        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        // Get recent calls from Twilio
        let response = self.make_twilio_request("Calls.json", json!({})).await?;

        let mut sessions = Vec::new();
        if let Some(calls) = response["calls"].as_array() {
            for call in calls {
                let call_sid = call["sid"].as_str().unwrap_or("");
                let status = match call["status"].as_str() {
                    Some("queued") | Some("ringing") => SessionStatus::Created,
                    Some("in-progress") => SessionStatus::Active,
                    Some("completed") | Some("busy") | Some("no-answer") | Some("canceled")
                    | Some("failed") => SessionStatus::Ended,
                    _ => SessionStatus::Created,
                };

                sessions.push(SessionInfo {
                    session_id: call_sid.to_string(),
                    room_name: call_sid.to_string(),
                    room_url: format!("twilio://call/{}", call_sid),
                    status,
                    participants: vec![],
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                });
            }
        }

        Ok(sessions)
    }

    async fn create_meeting_token(
        &self,
        session_id: &str,
        _participant: ParticipantConfig,
    ) -> Result<String> {
        // For Twilio, we don't need tokens in the same way as WebRTC
        // Return the call SID as the "token"
        Ok(session_id.to_string())
    }

    fn config_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "account_sid": {
                    "type": "string",
                    "description": "Twilio Account SID"
                },
                "auth_token": {
                    "type": "string",
                    "description": "Twilio Auth Token"
                },
                "phone_number": {
                    "type": "string",
                    "description": "Twilio Phone Number"
                },
                "webhook_url": {
                    "type": "string",
                    "description": "Webhook URL for call events"
                }
            },
            "required": ["account_sid", "auth_token", "phone_number"]
        })
    }
}

/// Factory function to create Twilio transport connector
pub fn twilio_transport_connector(config: TwilioConfig) -> std::sync::Arc<dyn TransportConnector> {
    std::sync::Arc::new(TwilioTransportConnector::new(config))
}
