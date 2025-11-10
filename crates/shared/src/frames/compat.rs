use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use uuid::Uuid;

use super::{AudioFormat, ControlFrame, DataFrame, FrameWrapper, SystemFrame};
use crate::queue::ProcessingEvent;

impl FrameWrapper {
    pub fn from_processing_event(event: ProcessingEvent) -> Result<Self> {
        let frame = match event.event_type.as_str() {
            "session_start" => FrameWrapper::System(SystemFrame::StartFrame {
                session_id: event.session_id.clone(),
                metadata: event
                    .payload
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            "session_end" => FrameWrapper::System(SystemFrame::EndFrame {
                session_id: event.session_id.clone(),
                reason: event
                    .payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            }),
            "interrupt_start" => FrameWrapper::System(SystemFrame::StartInterruptionFrame {
                session_id: event.session_id.clone(),
                source: event
                    .payload
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            }),
            "interrupt_stop" => FrameWrapper::System(SystemFrame::StopInterruptionFrame {
                session_id: event.session_id.clone(),
            }),
            "error" => FrameWrapper::System(SystemFrame::ErrorFrame {
                session_id: event.session_id.clone(),
                error: event
                    .payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
                recoverable: event
                    .payload
                    .get("recoverable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            }),
            "user_started_speaking" => {
                FrameWrapper::System(SystemFrame::UserStartedSpeakingFrame {
                    session_id: event.session_id.clone(),
                })
            }
            "user_stopped_speaking" => {
                FrameWrapper::System(SystemFrame::UserStoppedSpeakingFrame {
                    session_id: event.session_id.clone(),
                })
            }
            "audio_input" => {
                let audio_data_b64 = event
                    .payload
                    .get("audio_data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing audio_data"))?;
                let audio_data = STANDARD.decode(audio_data_b64)?;

                FrameWrapper::Data(DataFrame::InputAudioRawFrame {
                    session_id: event.session_id.clone(),
                    frame_id: Uuid::new_v4(),
                    timestamp: event.timestamp_ms,
                    audio_data,
                    sample_rate: event
                        .payload
                        .get("sample_rate")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(16000) as u32,
                    num_channels: event
                        .payload
                        .get("num_channels")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u16,
                    format: AudioFormat::PCM16,
                })
            }
            "audio_output" => {
                let audio_data_b64 = event
                    .payload
                    .get("audio_data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing audio_data"))?;
                let audio_data = STANDARD.decode(audio_data_b64)?;

                FrameWrapper::Data(DataFrame::OutputAudioRawFrame {
                    session_id: event.session_id.clone(),
                    frame_id: Uuid::new_v4(),
                    timestamp: event.timestamp_ms,
                    audio_data,
                    sample_rate: event
                        .payload
                        .get("sample_rate")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(24000) as u32,
                    num_channels: event
                        .payload
                        .get("num_channels")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u16,
                    format: AudioFormat::PCM16,
                })
            }
            "transcription_output" => FrameWrapper::Data(DataFrame::TranscriptionFrame {
                session_id: event.session_id.clone(),
                frame_id: Uuid::new_v4(),
                timestamp: event.timestamp_ms,
                text: event
                    .payload
                    .get("transcription")
                    .and_then(|t| t.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                language: event
                    .payload
                    .get("transcription")
                    .and_then(|t| t.get("language"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("en")
                    .to_string(),
                confidence: event
                    .payload
                    .get("transcription")
                    .and_then(|t| t.get("confidence"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0) as f32,
                is_final: event
                    .payload
                    .get("transcription")
                    .and_then(|t| t.get("is_final"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            }),
            "llm_response_complete" => FrameWrapper::Data(DataFrame::LLMTextFrame {
                session_id: event.session_id.clone(),
                frame_id: Uuid::new_v4(),
                timestamp: event.timestamp_ms,
                text: event
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                model: event
                    .payload
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                is_complete: true,
            }),
            "tts_started" => FrameWrapper::Control(ControlFrame::TTSStartedFrame {
                session_id: event.session_id.clone(),
                frame_id: Uuid::new_v4(),
                timestamp: event.timestamp_ms,
            }),
            "tts_stopped" => FrameWrapper::Control(ControlFrame::TTSStoppedFrame {
                session_id: event.session_id.clone(),
                frame_id: Uuid::new_v4(),
                timestamp: event.timestamp_ms,
            }),
            _ => {
                return Err(anyhow!("Unknown event type: {}", event.event_type));
            }
        };

        Ok(frame)
    }

    pub fn to_processing_event(&self) -> Result<ProcessingEvent> {
        let (session_id, event_type, payload, timestamp_ms) = match self {
            FrameWrapper::System(sys_frame) => {
                let session_id = sys_frame.session_id().to_string();
                let (event_type, payload) = match sys_frame {
                    SystemFrame::StartFrame { metadata, .. } => (
                        "session_start",
                        serde_json::to_value(metadata).unwrap_or_default(),
                    ),
                    SystemFrame::EndFrame { reason, .. } => (
                        "session_end",
                        serde_json::json!({"reason": reason}),
                    ),
                    SystemFrame::StartInterruptionFrame { source, .. } => (
                        "interrupt_start",
                        serde_json::json!({"source": source}),
                    ),
                    SystemFrame::StopInterruptionFrame { .. } => {
                        ("interrupt_stop", serde_json::json!({}))
                    }
                    SystemFrame::ErrorFrame {
                        error, recoverable, ..
                    } => (
                        "error",
                        serde_json::json!({"error": error, "recoverable": recoverable}),
                    ),
                    SystemFrame::UserStartedSpeakingFrame { .. } => {
                        ("user_started_speaking", serde_json::json!({}))
                    }
                    SystemFrame::UserStoppedSpeakingFrame { .. } => {
                        ("user_stopped_speaking", serde_json::json!({}))
                    }
                    SystemFrame::CancelFrame { .. } => ("cancel", serde_json::json!({})),
                };
                (session_id, event_type, payload, chrono::Utc::now().timestamp_millis() as u64)
            }
            FrameWrapper::Data(data_frame) => {
                let session_id = data_frame.session_id().to_string();
                let timestamp_ms = data_frame.timestamp();
                let (event_type, payload) = match data_frame {
                    DataFrame::InputAudioRawFrame {
                        audio_data,
                        sample_rate,
                        num_channels,
                        ..
                    } => (
                        "audio_input",
                        serde_json::json!({
                            "audio_data": STANDARD.encode(audio_data),
                            "sample_rate": sample_rate,
                            "num_channels": num_channels,
                        }),
                    ),
                    DataFrame::OutputAudioRawFrame {
                        audio_data,
                        sample_rate,
                        num_channels,
                        ..
                    } => (
                        "audio_output",
                        serde_json::json!({
                            "audio_data": STANDARD.encode(audio_data),
                            "sample_rate": sample_rate,
                            "num_channels": num_channels,
                        }),
                    ),
                    DataFrame::TranscriptionFrame {
                        text,
                        language,
                        confidence,
                        is_final,
                        ..
                    } => (
                        "transcription_output",
                        serde_json::json!({
                            "transcription": {
                                "text": text,
                                "language": language,
                                "confidence": confidence,
                                "is_final": is_final,
                            }
                        }),
                    ),
                    DataFrame::LLMTextFrame { text, model, .. } => (
                        "llm_response_complete",
                        serde_json::json!({
                            "text": text,
                            "model": model,
                        }),
                    ),
                    DataFrame::TextFrame { text, language, .. } => (
                        "text",
                        serde_json::json!({
                            "text": text,
                            "language": language,
                        }),
                    ),
                    DataFrame::ImageFrame { image_data, .. } => (
                        "image",
                        serde_json::json!({
                            "image_data": STANDARD.encode(image_data),
                        }),
                    ),
                };
                (session_id, event_type, payload, timestamp_ms)
            }
            FrameWrapper::Control(control_frame) => {
                let session_id = control_frame.session_id().to_string();
                let timestamp_ms = control_frame.timestamp();
                let event_type = match control_frame {
                    ControlFrame::TTSStartedFrame { .. } => "tts_started",
                    ControlFrame::TTSStoppedFrame { .. } => "tts_stopped",
                    ControlFrame::LLMFullResponseStartFrame { .. } => "llm_response_start",
                    ControlFrame::LLMFullResponseEndFrame { .. } => "llm_response_end",
                    ControlFrame::BotStartedSpeakingFrame { .. } => "bot_started_speaking",
                    ControlFrame::BotStoppedSpeakingFrame { .. } => "bot_stopped_speaking",
                };
                (session_id, event_type, serde_json::json!({}), timestamp_ms)
            }
        };

        Ok(ProcessingEvent {
            session_id,
            conversation_id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            payload,
            timestamp_ms,
        })
    }
}
