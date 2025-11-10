use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControlFrame {
    TTSStartedFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
    TTSStoppedFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
    LLMFullResponseStartFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
    LLMFullResponseEndFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
    BotStartedSpeakingFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
    BotStoppedSpeakingFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
    },
}

impl ControlFrame {
    pub fn session_id(&self) -> &str {
        match self {
            ControlFrame::TTSStartedFrame { session_id, .. } => session_id,
            ControlFrame::TTSStoppedFrame { session_id, .. } => session_id,
            ControlFrame::LLMFullResponseStartFrame { session_id, .. } => session_id,
            ControlFrame::LLMFullResponseEndFrame { session_id, .. } => session_id,
            ControlFrame::BotStartedSpeakingFrame { session_id, .. } => session_id,
            ControlFrame::BotStoppedSpeakingFrame { session_id, .. } => session_id,
        }
    }

    pub fn frame_id(&self) -> Uuid {
        match self {
            ControlFrame::TTSStartedFrame { frame_id, .. } => *frame_id,
            ControlFrame::TTSStoppedFrame { frame_id, .. } => *frame_id,
            ControlFrame::LLMFullResponseStartFrame { frame_id, .. } => *frame_id,
            ControlFrame::LLMFullResponseEndFrame { frame_id, .. } => *frame_id,
            ControlFrame::BotStartedSpeakingFrame { frame_id, .. } => *frame_id,
            ControlFrame::BotStoppedSpeakingFrame { frame_id, .. } => *frame_id,
        }
    }

    pub fn timestamp(&self) -> u64 {
        match self {
            ControlFrame::TTSStartedFrame { timestamp, .. } => *timestamp,
            ControlFrame::TTSStoppedFrame { timestamp, .. } => *timestamp,
            ControlFrame::LLMFullResponseStartFrame { timestamp, .. } => *timestamp,
            ControlFrame::LLMFullResponseEndFrame { timestamp, .. } => *timestamp,
            ControlFrame::BotStartedSpeakingFrame { timestamp, .. } => *timestamp,
            ControlFrame::BotStoppedSpeakingFrame { timestamp, .. } => *timestamp,
        }
    }
}
