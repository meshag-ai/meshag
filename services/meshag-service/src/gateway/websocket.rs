use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use meshag_shared::{FrameWrapper, SubjectName};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::state::GatewayState;

pub async fn handle_websocket(socket: WebSocket, state: Arc<GatewayState>) {
    let session_id = Uuid::new_v4();
    info!(session_id = %session_id, "WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(100);

    state.register_session(session_id);

    let system_subject = SubjectName::SystemSubject.as_str(Some(session_id.to_string()));
    let data_subject = SubjectName::DataSubject.as_str(Some(session_id.to_string()));

    let state_clone = state.clone();
    let system_subject_clone = system_subject.to_string();
    let data_subject_clone = data_subject.to_string();

    tokio::spawn(async move {
        let mut system_sub = match state_clone
            .event_queue
            .subscribe(&system_subject_clone)
            .await
        {
            Ok(sub) => sub,
            Err(e) => {
                error!("Failed to subscribe to system subject: {}", e);
                return;
            }
        };

        let mut data_sub = match state_clone.event_queue.subscribe(&data_subject_clone).await {
            Ok(sub) => sub,
            Err(e) => {
                error!("Failed to subscribe to data subject: {}", e);
                return;
            }
        };

        loop {
            tokio::select! {
                biased;

                Some(msg) = system_sub.next() => {
                    if let Ok(frame) = FrameWrapper::from_bytes(&msg.payload) {
                        if let Ok(json) = frame.to_json() {
                            if tx.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                }

                Some(msg) = data_sub.next() => {
                    if let Ok(frame) = FrameWrapper::from_bytes(&msg.payload) {
                        if let Ok(json) = frame.to_json() {
                            if tx.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    let queue = state.event_queue.clone();
    tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => match FrameWrapper::from_json(&text) {
                    Ok(frame) => {
                        let subject = match frame.frame_category() {
                            meshag_shared::FrameCategory::System => &system_subject,
                            _ => &data_subject,
                        };

                        if let Ok(bytes) = frame.to_bytes() {
                            if let Err(e) = queue.publish_bytes(subject, bytes).await {
                                error!("Failed to publish frame: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse frame from WebSocket: {}", e);
                    }
                },
                Ok(Message::Binary(bytes)) => match FrameWrapper::from_bytes(&bytes) {
                    Ok(frame) => {
                        let subject = match frame.frame_category() {
                            meshag_shared::FrameCategory::System => &system_subject,
                            _ => &data_subject,
                        };

                        if let Err(e) = queue.publish_bytes(subject, bytes).await {
                            error!("Failed to publish frame: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse binary frame: {}", e);
                    }
                },
                Ok(Message::Close(_)) => {
                    info!(session_id = %session_id, "WebSocket closed by client");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    while let Some(msg) = rx.recv().await {
        if ws_sender.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }

    state.unregister_session(&session_id);
    info!(session_id = %session_id, "WebSocket connection closed");
}
