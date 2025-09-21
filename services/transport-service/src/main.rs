use anyhow::Result;
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use meshag_transport_service::{
    CreateSessionRequest, TransportServiceConfig, TransportServiceState, WebSocketConnection,
    WebSocketMessage,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Load configuration
    let config = TransportServiceConfig::from_env()?;
    info!(
        "Starting transport service with Daily domain: {}",
        config.daily_domain
    );

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
        // Session management endpoints
        .route("/sessions", post(create_session))
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
async fn handle_websocket(
    socket: axum::extract::ws::WebSocket,
    _state: Arc<TransportServiceState>,
) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let _connection = WebSocketConnection::new();

    info!("WebSocket connection established");

    // Send welcome message
    let welcome_msg = WebSocketMessage::Ping;
    if let Ok(msg_text) = serde_json::to_string(&welcome_msg) {
        if sender.send(Message::Text(msg_text)).await.is_err() {
            error!("Failed to send welcome message");
            return;
        }
    }

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(ws_msg) = serde_json::from_str::<WebSocketMessage>(&text) {
                    match ws_msg {
                        WebSocketMessage::Ping => {
                            let pong = WebSocketMessage::Pong;
                            if let Ok(pong_text) = serde_json::to_string(&pong) {
                                if sender.send(Message::Text(pong_text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        _ => {
                            info!("Received WebSocket message: {:?}", ws_msg);
                            // Handle other message types as needed
                        }
                    }
                } else {
                    error!("Failed to parse WebSocket message: {}", text);
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket connection closed");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {
                // Handle other message types (binary, ping, pong)
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
