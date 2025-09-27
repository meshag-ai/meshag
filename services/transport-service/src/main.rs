use anyhow::Result;
use axum::{
    extract::{Form, Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};

use meshag_orchestrator::AgentConfig;
use meshag_shared::TwilioMediaFormat;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use transport_service::{
    CreateSessionRequest, TransportServiceConfig, TransportServiceState, WebSocketConnection,
};

/// Request to create a session with configuration
#[derive(Debug, Deserialize)]
struct CreateSessionWithConfigRequest {
    pub config: AgentConfig,
    pub session_name: Option<String>,
    pub max_participants: Option<u32>,
    pub enable_recording: Option<bool>,
    pub enable_transcription: Option<bool>,
}

/// Response for session creation with config
#[derive(Debug, Serialize)]
struct CreateSessionWithConfigResponse {
    pub session_id: String,
    pub room_url: String,
    pub token: String,
    pub config_stored: bool,
}

#[derive(Debug, Deserialize)]
struct TwilioWebhookData {
    #[serde(rename = "CallSid")]
    pub call_sid: String,
    #[serde(rename = "From")]
    pub from: String,
    #[serde(rename = "To")]
    pub to: String,
    #[serde(rename = "CallStatus")]
    pub call_status: String,
    #[serde(rename = "Direction")]
    pub direction: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Load configuration
    let config = TransportServiceConfig::from_env()?;
    info!("Starting transport service");

    // Initialize service state
    let state = TransportServiceState::new(config).await?;
    info!("Transport service initialized successfully");

    // Create the application router
    let app = create_app_router(state.clone());

    // Start the HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting transport service on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_app_router(state: Arc<TransportServiceState>) -> Router {
    Router::new()
        // WebSocket endpoint
        .route("/ws", get(websocket_handler))
        // Twilio endpoint
        .route("/twilio", post(handle_twilio_session))
        // Session management endpoints
        .route("/sessions", post(create_session))
        .route("/sessions/with-config", post(create_session_with_config))
        .route("/sessions", get(list_sessions))
        .route("/sessions/:session_id", get(get_session))
        .route("/sessions/:session_id", delete(end_session))
        // Health endpoints (from service-common)
        .route(
            "/health",
            get(meshag_service_common::handlers::health_check::<TransportServiceState>),
        )
        .route(
            "/ready",
            get(meshag_service_common::handlers::readiness_check::<TransportServiceState>),
        )
        .route(
            "/metrics",
            get(meshag_service_common::handlers::metrics::<TransportServiceState>),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Create a new session
async fn create_session(
    State(state): State<Arc<TransportServiceState>>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!("Creating session with provider: {}", request.provider);

    let response = state.create_session(request).await?;

    Ok(Json(json!({
        "session_id": response.session_id,
        "room_url": response.room_url,
        "room_name": response.room_name,
        "meeting_token": response.meeting_token,
        "expires_at": response.expires_at,
        "provider_metadata": response.provider_metadata
    })))
}

async fn handle_twilio_session(
    State(_state): State<Arc<TransportServiceState>>,
    Form(webhook_data): Form<TwilioWebhookData>,
) -> Result<Response, AppError> {
    // Parse Twilio webhook data
    tracing::info!(
        "Received Twilio webhook: CallSid={}, From={}, To={}, Status={}",
        webhook_data.call_sid,
        webhook_data.from,
        webhook_data.to,
        webhook_data.call_status
    );

    let xml_response = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
    <Connect>
        <Stream url="wss://semiadhesive-stephane-uninchoative.ngrok-free.dev/ws"/>
    </Connect>
</Response>"#;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml")
        .body(xml_response.into())
        .unwrap())
}

/// Get session information
async fn get_session(
    State(state): State<Arc<TransportServiceState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state.get_session(&session_id).await?;

    Ok(Json(json!({
        "session_id": session.session_id,
        "room_name": session.room_name,
        "room_url": session.room_url,
        "status": format!("{:?}", session.status),
        "participants": session.participants,
        "created_at": session.created_at,
        "updated_at": session.updated_at
    })))
}

/// End a session
async fn end_session(
    State(state): State<Arc<TransportServiceState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.end_session(&session_id).await?;

    Ok(Json(json!({
        "message": "Session ended successfully",
        "session_id": session_id
    })))
}

/// List all sessions
async fn list_sessions(
    State(state): State<Arc<TransportServiceState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sessions = state.list_sessions().await?;

    // Optional filtering by status
    let filtered_sessions: Vec<_> = if let Some(status_filter) = params.get("status") {
        sessions
            .into_iter()
            .filter(|s| format!("{:?}", s.status).to_lowercase() == status_filter.to_lowercase())
            .collect()
    } else {
        sessions
    };

    Ok(Json(json!({
        "sessions": filtered_sessions.iter().map(|s| json!({
            "session_id": s.session_id,
            "room_name": s.room_name,
            "room_url": s.room_url,
            "status": format!("{:?}", s.status),
            "participants": s.participants,
            "created_at": s.created_at,
            "updated_at": s.updated_at
        })).collect::<Vec<_>>(),
        "total": filtered_sessions.len()
    })))
}

/// WebSocket handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_state): State<Arc<TransportServiceState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, _state))
}

/// Handle WebSocket connections
async fn handle_websocket(socket: axum::extract::ws::WebSocket, state: Arc<TransportServiceState>) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    let (mut sender, mut receiver) = socket.split();
    let _connection = WebSocketConnection::new();

    // Store session information for media events
    let mut session_info: HashMap<String, serde_json::Value> = HashMap::new();
    let mut media_format: Option<TwilioMediaFormat> = None;
    let mut session_id: Option<String> = None;

    // Channel for NATS responses to WebSocket
    let (tx, mut rx) = mpsc::channel::<String>(100);

    info!("WebSocket connection established");

    // Start NATS consumer task
    let state_clone = state.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let event_queue = state_clone.event_queue.clone();
        let stream_config = meshag_shared::StreamConfig::tts_output();
        let subject = "TTS_OUTPUT".to_string();

        if let Err(e) = event_queue.consume_events(stream_config, subject, move |event| {
            let tx = tx_clone.clone();

            async move {
                if let Some(payload) = event.payload.get("session_id") {
                    if let Some(session_id) = payload.as_str() {
                        // Parse response payload
                        if let Ok(response_payload) = serde_json::from_value::<transport_service::ResponseEventPayload>(event.payload.clone()) {
                            tracing::info!(
                                session_id = %session_id,
                                response_type = %response_payload.response_type,
                                "Received response event for WebSocket"
                            );

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

                            if let Ok(message_text) = serde_json::to_string(&media_message) {
                                if let Err(e) = tx.send(message_text).await {
                                    tracing::error!("Failed to send response to WebSocket channel: {}", e);
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
        }).await {
            tracing::error!("NATS consumer error: {}", e);
        }
    });

    // Handle both WebSocket messages and NATS responses concurrently
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // First try to parse as Twilio event
                        if let Ok(twilio_event) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(event_type) = twilio_event.get("event").and_then(|e| e.as_str()) {
                                match event_type {
                                    "connected" => {
                                        info!(
                                            "Twilio Stream Connected: protocol={}, version={}",
                                            twilio_event
                                                .get("protocol")
                                                .and_then(|p| p.as_str())
                                                .unwrap_or("unknown"),
                                            twilio_event
                                                .get("version")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown")
                                        );
                                    }
                                    "start" => {
                                        if let Some(start_data) = twilio_event.get("start") {
                                            let call_sid = start_data
                                                .get("callSid")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("unknown");
                                            let stream_sid = start_data
                                                .get("streamSid")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("unknown");

                                            info!("Twilio Stream Started: CallSid={}, StreamSid={}, Tracks={:?}",
                                                call_sid,
                                                stream_sid,
                                                start_data.get("tracks").and_then(|t| t.as_array()).map(|arr|
                                                    arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()
                                                ).unwrap_or_default()
                                            );

                                            // Store session information
                                            session_info.insert("call_sid".to_string(), call_sid.into());
                                            session_info
                                                .insert("stream_sid".to_string(), stream_sid.into());

                                            // Generate session ID for NATS events
                                            session_id = Some(format!("ws_{}", stream_sid));

                                            // Parse and store media format
                                            if let Some(format_data) = start_data.get("mediaFormat") {
                                                if let Ok(format) = serde_json::from_value::<
                                                    TwilioMediaFormat,
                                                >(
                                                    format_data.clone()
                                                ) {
                                                    info!("Media format: encoding={}, sample_rate={}, channels={}",
                                                        format.encoding, format.sample_rate, format.channels);
                                                    media_format = Some(format);
                                                }
                                            }
                                        }
                                    }
                                    "media" => {
                                        if let Some(media_data) = twilio_event.get("media") {
                                            let track = media_data
                                                .get("track")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("unknown");
                                            let chunk = media_data
                                                .get("chunk")
                                                .and_then(|c| c.as_str())
                                                .unwrap_or("unknown");
                                            let timestamp = media_data
                                                .get("timestamp")
                                                .and_then(|ts| ts.as_str())
                                                .unwrap_or("unknown");
                                            let payload = media_data
                                                .get("payload")
                                                .and_then(|p| p.as_str())
                                                .unwrap_or("");


                                            // Publish media event to STT service if we have session info and media format
                                            if let (Some(call_sid), Some(stream_sid), Some(format), Some(session_id)) = (
                                                session_info.get("call_sid").and_then(|v| v.as_str()),
                                                session_info.get("stream_sid").and_then(|v| v.as_str()),
                                                &media_format,
                                                &session_id,
                                            ) {
                                                if let Err(e) = state
                                                    .publish_media_event(
                                                        session_id,
                                                        call_sid,
                                                        stream_sid,
                                                        track,
                                                        chunk,
                                                        timestamp,
                                                        payload,
                                                        format.clone(),
                                                    )
                                                    .await
                                                {
                                                    error!("Failed to publish media event: {}", e);
                                                }
                                            } else {
                                                // warn!("Received media event but missing session info or media format");
                                            }
                                        }
                                    }
                                    _ => {
                                        info!("Received unknown Twilio event: {}", event_type);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket connection ended");
                        break;
                    }
                    _ => {}
                }
            }
            // Handle NATS responses
            response = rx.recv() => {
                match response {
                    Some(message_text) => {
                        if let Err(e) = sender.send(Message::Text(message_text)).await {
                            error!("Failed to send NATS response to WebSocket: {}", e);
                            break;
                        }
                    }
                    None => {
                        info!("NATS response channel closed");
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket connection ended");
}

/// Application error type
#[derive(Debug)]
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!("Application error: {}", self.0);

        let error_response = json!({
            "error": "Internal server error",
            "message": self.0.to_string()
        });

        (StatusCode::INTERNAL_SERVER_ERROR, Json(error_response)).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// Create a session with configuration
async fn create_session_with_config(
    State(state): State<Arc<TransportServiceState>>,
    Json(request): Json<CreateSessionWithConfigRequest>,
) -> Result<Json<CreateSessionWithConfigResponse>, AppError> {
    // Validate the configuration
    request.config.validate().map_err(AppError::from)?;

    // Create the session
    let session_request = CreateSessionRequest {
        provider: "daily".to_string(),
        room_config: meshag_connectors::RoomConfig {
            name: request.session_name,
            privacy: meshag_connectors::RoomPrivacy::Public,
            max_participants: request.max_participants,
            enable_recording: request.enable_recording.unwrap_or(true),
            enable_transcription: request.enable_transcription.unwrap_or(true),
            enable_chat: true,
            eject_after_elapsed: Some(60), // Auto-eject after 60 seconds (1 minute) of inactivity
            eject_at_room_exp: Some(true), // Eject all participants when room expires
        },
        participant_config: meshag_connectors::ParticipantConfig {
            name: Some("agent".to_string()),
            is_owner: true,
            permissions: meshag_connectors::ParticipantPermissions {
                can_admin: true,
                can_send_video: true,
                can_send_audio: true,
                can_send_screen_video: false,
                can_send_screen_audio: false,
            },
        },
        options: std::collections::HashMap::new(),
    };

    let session_response = state.create_session(session_request).await?;

    // Store the configuration in Valkey
    let config_stored = state
        .store_config(&session_response.session_id, &request.config)
        .await
        .is_ok();

    if !config_stored {
        error!(
            "Failed to store configuration for session: {}",
            session_response.session_id
        );
    }

    // Automatically join the AI agent to the session and start processing
    if let Err(e) = state
        .join_as_ai_agent(&session_response.session_id, "AI Agent")
        .await
    {
        error!("Failed to join AI agent to session: {}", e);
    }

    let response = CreateSessionWithConfigResponse {
        session_id: session_response.session_id,
        room_url: session_response.room_url,
        token: session_response.meeting_token.unwrap_or_default(),
        config_stored,
    };

    info!(
        "Created session {} with config stored: {}",
        response.session_id, config_stored
    );

    Ok(Json(response))
}
