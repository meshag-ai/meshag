use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use meshag_shared::{FrameCategory, FrameWrapper, SubjectName, SystemFrame};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

use crate::state::GatewayState;

pub async fn handle_websocket(socket: WebSocket, state: Arc<GatewayState>) {
    let session_id = Uuid::new_v4();
    info!(session_id = %session_id, "WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(100);

    state.register_session(session_id);

    let system_subject = SubjectName::SystemSubject.as_str(Some(session_id.to_string()));
    let data_subject = SubjectName::DataSubject.as_str(Some(session_id.to_string()));

    let state_clone = state.clone();
    let tx_clone = tx.clone();

    tokio::spawn(async move {
        let mut system_sub = match state_clone
            .event_queue
            .subscribe(system_subject)
            .await
        {
            Ok(sub) => sub,
            Err(e) => {
                error!("Failed to subscribe to system subject: {}", e);
                return;
            }
        };

        let mut data_sub = match state_clone.event_queue.subscribe(data_subject).await {
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
                            if tx_clone.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                }

                Some(msg) = data_sub.next() => {
                    if let Ok(frame) = FrameWrapper::from_bytes(&msg.payload) {
                        if let Ok(json) = frame.to_json() {
                            if tx_clone.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        while let Some(message_text) = rx.recv().await {
            if ws_sender
                .send(Message::Text(message_text))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let start_frame = FrameWrapper::System(SystemFrame::StartFrame {
        session_id: session_id.to_string(),
        metadata: HashMap::new(),
    });

    if let Ok(bytes) = start_frame.to_bytes() {
        let subject = SubjectName::SystemSubject.as_str(Some(session_id.to_string()));
        let _ = state.event_queue.publish_bytes(subject, bytes).await;
    }

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match FrameWrapper::from_json(&text) {
                    Ok(frame) => {
                        let subject = match frame.frame_category() {
                            FrameCategory::System => SubjectName::SystemSubject
                                .as_str(Some(session_id.to_string())),
                            _ => SubjectName::DataSubject.as_str(Some(session_id.to_string())),
                        };

                        match frame.to_bytes() {
                            Ok(bytes) => {
                                if let Err(e) =
                                    state.event_queue.publish_bytes(subject, bytes).await
                                {
                                    error!("Failed to publish frame: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to serialize frame: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse frame: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!(session_id = %session_id, "WebSocket connection closed by client");
                break;
            }
            Err(e) => {
                error!(session_id = %session_id, "WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    let end_frame = FrameWrapper::System(SystemFrame::EndFrame {
        session_id: session_id.to_string(),
        reason: "Connection closed".to_string(),
    });

    if let Ok(bytes) = end_frame.to_bytes() {
        let subject = SubjectName::SystemSubject.as_str(Some(session_id.to_string()));
        let _ = state.event_queue.publish_bytes(subject, bytes).await;
    }

    state.unregister_session(&session_id);
    info!(session_id = %session_id, "WebSocket connection ended");
}
