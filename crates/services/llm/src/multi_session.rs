use anyhow::Result;
use dashmap::DashMap;
use meshag_shared::{ControlFrame, DataFrame, EventQueue, FrameWrapper, SubjectName, SystemFrame};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::LlmService;

struct SessionChannels {
    system_tx: mpsc::Sender<FrameWrapper>,
    data_tx: mpsc::Sender<FrameWrapper>,
    _cancel_token: CancellationToken,
}

pub struct MultiSessionLlmService {
    llm_service: Arc<LlmService>,
    sessions: Arc<DashMap<String, SessionChannels>>,
}

impl MultiSessionLlmService {
    pub fn new(llm_service: LlmService) -> Self {
        Self {
            llm_service: Arc::new(llm_service),
            sessions: Arc::new(DashMap::new()),
        }
    }

    pub async fn start_dispatcher(&self, queue: EventQueue) -> Result<()> {
        let sessions = self.sessions.clone();
        let llm_service = self.llm_service.clone();

        tokio::spawn(async move {
            let system_subject = SubjectName::SystemSubject.as_str(None);
            let data_subject = SubjectName::DataSubject.as_str(None);

            let mut system_sub = match queue.subscribe(system_subject).await {
                Ok(sub) => sub,
                Err(e) => {
                    error!("Failed to subscribe to system subject: {}", e);
                    return;
                }
            };

            let mut data_sub = match queue.subscribe(data_subject).await {
                Ok(sub) => sub,
                Err(e) => {
                    error!("Failed to subscribe to data subject: {}", e);
                    return;
                }
            };

            loop {
                tokio::select! {
                    biased;

                    Some(msg) = futures::StreamExt::next(&mut system_sub) => {
                        if let Ok(frame) = FrameWrapper::from_bytes(&msg.payload) {
                            let session_id = frame.session_id().to_string();

                            let channels = sessions.entry(session_id.clone())
                                .or_insert_with(|| {
                                    Self::spawn_session_handler(
                                        session_id.clone(),
                                        llm_service.clone(),
                                        queue.clone(),
                                    )
                                });

                            let _ = channels.system_tx.send(frame).await;
                        }
                    }

                    Some(msg) = futures::StreamExt::next(&mut data_sub) => {
                        if let Ok(frame) = FrameWrapper::from_bytes(&msg.payload) {
                            if !Self::can_process(&frame) {
                                continue;
                            }

                            let session_id = frame.session_id().to_string();

                            let channels = sessions.entry(session_id.clone())
                                .or_insert_with(|| {
                                    Self::spawn_session_handler(
                                        session_id.clone(),
                                        llm_service.clone(),
                                        queue.clone(),
                                    )
                                });

                            let _ = channels.data_tx.send(frame).await;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    fn can_process(frame: &FrameWrapper) -> bool {
        matches!(
            frame,
            FrameWrapper::Data(DataFrame::TranscriptionFrame { .. })
        )
    }

    fn spawn_session_handler(
        session_id: String,
        llm_service: Arc<LlmService>,
        queue: EventQueue,
    ) -> SessionChannels {
        let (system_tx, mut system_rx) = mpsc::channel(100);
        let (data_tx, mut data_rx) = mpsc::channel(100);
        let cancel_token = CancellationToken::new();

        let cancel_clone = cancel_token.clone();

        tokio::spawn(async move {
            let mut interrupted = false;

            info!(session_id = %session_id, "LLM session handler started");

            loop {
                tokio::select! {
                    biased;

                    Some(frame) = system_rx.recv() => {
                        match frame {
                            FrameWrapper::System(SystemFrame::StartInterruptionFrame { .. }) => {
                                info!(session_id = %session_id, "LLM session interrupted");
                                interrupted = true;
                                while data_rx.try_recv().is_ok() {}
                            }
                            FrameWrapper::System(SystemFrame::StopInterruptionFrame { .. }) => {
                                info!(session_id = %session_id, "LLM session interrupt stopped");
                                interrupted = false;
                            }
                            FrameWrapper::System(SystemFrame::EndFrame { .. }) => {
                                info!(session_id = %session_id, "LLM session ended");
                                break;
                            }
                            _ => {}
                        }
                    }

                    Some(frame) = data_rx.recv(), if !interrupted => {
                        if let FrameWrapper::Data(DataFrame::TranscriptionFrame {
                            text,
                            session_id: sid,
                            ..
                        }) = frame {
                            if let Err(e) = Self::process_transcription(
                                &llm_service,
                                &queue,
                                sid,
                                text,
                            ).await {
                                error!(session_id = %session_id, error = %e, "Failed to process transcription");
                            }
                        }
                    }

                    _ = cancel_clone.cancelled() => {
                        info!(session_id = %session_id, "LLM session cancelled");
                        break;
                    }
                }
            }
        });

        SessionChannels {
            system_tx,
            data_tx,
            _cancel_token: cancel_token,
        }
    }

    async fn process_transcription(
        llm_service: &Arc<LlmService>,
        queue: &EventQueue,
        session_id: String,
        text: String,
    ) -> Result<()> {
        use meshag_connectors::{ChatMessage, LlmRequest, MessageRole};
        use std::collections::HashMap;
        use uuid::Uuid;

        let user_message = ChatMessage {
            role: MessageRole::User,
            content: text,
            timestamp: chrono::Utc::now(),
        };
        llm_service.add_message(session_id.clone(), user_message).await;

        let connector = match llm_service.get_connector(None).await {
            Some(c) => c,
            None => {
                error!("No LLM connector available");
                return Ok(());
            }
        };

        let messages = llm_service.get_conversation(session_id.clone()).await;

        let llm_request = LlmRequest {
            messages,
            model: None,
            temperature: None,
            max_tokens: None,
            system_prompt: None,
            options: HashMap::new(),
        };

        let response_start = FrameWrapper::Control(ControlFrame::LLMFullResponseStartFrame {
            session_id: session_id.clone(),
            frame_id: Uuid::new_v4(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });

        let subject = SubjectName::DataSubject.as_str(Some(session_id.clone()));
        if let Ok(bytes) = response_start.to_bytes() {
            let _ = queue.publish_bytes(subject, bytes).await;
        }

        match connector.generate(llm_request).await {
            Ok(response) => {
                let assistant_message = ChatMessage {
                    role: MessageRole::Assistant,
                    content: response.text.clone(),
                    timestamp: chrono::Utc::now(),
                };
                llm_service
                    .add_message(session_id.clone(), assistant_message)
                    .await;

                let output_frame = FrameWrapper::Data(DataFrame::LLMTextFrame {
                    session_id: session_id.clone(),
                    frame_id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    text: response.text,
                    model: response.model_used,
                    is_complete: true,
                });

                if let Ok(bytes) = output_frame.to_bytes() {
                    queue.publish_bytes(subject, bytes).await?;
                }

                let response_end = FrameWrapper::Control(ControlFrame::LLMFullResponseEndFrame {
                    session_id: session_id.clone(),
                    frame_id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });

                if let Ok(bytes) = response_end.to_bytes() {
                    let _ = queue.publish_bytes(subject, bytes).await;
                }
            }
            Err(e) => {
                error!(session_id = %session_id, error = %e, "LLM generation failed");

                let error_frame = FrameWrapper::System(SystemFrame::ErrorFrame {
                    session_id: session_id.clone(),
                    error: format!("LLM generation failed: {}", e),
                    recoverable: true,
                });

                let sys_subject = SubjectName::SystemSubject.as_str(Some(session_id.clone()));
                if let Ok(bytes) = error_frame.to_bytes() {
                    let _ = queue.publish_bytes(sys_subject, bytes).await;
                }
            }
        }

        Ok(())
    }
}
