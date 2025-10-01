use anyhow::Result;
use axum::{
    extract::{Form, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use meshag_shared::{ProcessingEvent, TwilioMediaFormat};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use transport_service::{TransportServiceConfig, TransportServiceState, WebSocketConnection};
use uuid::Uuid;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();

    let config = TransportServiceConfig::from_env()?;
    info!("Starting transport service");

    let state = TransportServiceState::new(config).await?;
    info!("Transport service initialized successfully");

    let app = create_app_router(state.clone());

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
        .route("/ws", get(websocket_handler))
        .route("/twilio", post(handle_twilio_session))
        .route(
            "/health",
            get(meshag_service_common::handlers::health_check::<TransportServiceState>),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn handle_twilio_session(
    State(_state): State<Arc<TransportServiceState>>,
    Form(webhook_data): Form<TwilioWebhookData>,
) -> Result<Response, AppError> {
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

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_state): State<Arc<TransportServiceState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, _state))
}

async fn handle_websocket(socket: axum::extract::ws::WebSocket, state: Arc<TransportServiceState>) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    let session_id = uuid::Uuid::new_v4();

    let (mut sender, mut receiver) = socket.split();
    let _connection = WebSocketConnection::new();

    let session_info: Arc<Mutex<HashMap<String, serde_json::Value>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut media_format: Option<TwilioMediaFormat> = None;

    let (tx, mut rx) = mpsc::channel::<String>(100);

    info!("WebSocket connection established");

    let state_clone = state.clone();
    let tx_clone = tx.clone();
    let session_info_clone = session_info.clone();

    tokio::spawn(async move {
        let event_queue = state_clone.event_queue.clone();
        let subject = format!("TTS_OUTPUT.session.{}", session_id);
        let event = ProcessingEvent {
            session_id: session_id.to_string(),
            conversation_id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            event_type: "session_start".to_string(),
            payload: serde_json::Value::Null,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            source_service: "transport-service".to_string(),
            target_service: "transport-service".to_string(),
        };

        let _ = event_queue.publish_nats_core(&subject, event).await;
        if let Err(e) = event_queue
            .subscribe_nats_core(subject, move |event| {
                let tx = tx_clone.clone();
                let session_info = session_info_clone.clone();

                async move {
                    match serde_json::from_value::<transport_service::EventPayload>(
                        event.payload.clone(),
                    ) {
                        Ok(response_payload) => {
                            let stream_sid = {
                                let session_info_guard = session_info.lock().unwrap();
                                session_info_guard
                                    .get("stream_sid")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string()
                            };

                            tracing::info!(
                                session_id = %event.session_id,
                                response_type = %event.event_type,
                                stream_sid = stream_sid,
                                "Received response event for WebSocket"
                            );

                            let media_message = serde_json::json!({
                                "event": "media",
                                "streamSid": stream_sid,
                                "media": {
                                    "track": "outbound",
                                    "chunk": "1",
                                    "timestamp": chrono::Utc::now().timestamp_millis().to_string(),
                                    "payload": STANDARD.encode(&response_payload.audio_data)
                                }
                            });

                            match serde_json::to_string(&media_message) {
                                Ok(message_text) => match tx.send(message_text).await {
                                    Ok(_) => {
                                        tracing::debug!(
                                            "Successfully sent response to WebSocket channel"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to send response to WebSocket channel: {}",
                                            e
                                        );
                                    }
                                },
                                Err(e) => {
                                    tracing::error!("Failed to serialize media message: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to deserialize response event: {}", e);
                        }
                    }

                    Ok(())
                }
            })
            .await
        {
            tracing::error!("NATS consumer error: {}", e);
        }
    });

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
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

                                            {
                                                let mut session_info_guard = session_info.lock().unwrap();
                                                session_info_guard.insert("call_sid".to_string(), call_sid.into());
                                                session_info_guard.insert("stream_sid".to_string(), stream_sid.into());
                                            }


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
                                            if let Err(e) = state
                                                .publish_session_start_event(&session_id.to_string())
                                                .await
                                            {
                                                error!("Failed to publish session start event: {}", e);
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


                                            let (call_sid, stream_sid) = {
                                                let session_info_guard = session_info.lock().unwrap();
                                                (
                                                    session_info_guard.get("call_sid").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                    session_info_guard.get("stream_sid").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                )
                                            };

                                            if let (Some(call_sid), Some(stream_sid), Some(format)) = (
                                                call_sid.as_deref(),
                                                stream_sid.as_deref(),
                                                &media_format,
                                            ) {
                                                if let Err(e) = state
                                                    .publish_media_event(
                                                        &session_id.to_string(),
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
                        if let Err(e) = state
                            .publish_session_end_event(&session_id.to_string())
                            .await
                        {
                            error!("Failed to publish session end event: {}", e);
                        }
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
