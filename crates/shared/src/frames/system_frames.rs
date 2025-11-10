use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemFrame {
    StartFrame {
        session_id: String,
        metadata: HashMap<String, Value>,
    },
    EndFrame {
        session_id: String,
        reason: String,
    },
    StartInterruptionFrame {
        session_id: String,
        source: String,
    },
    StopInterruptionFrame {
        session_id: String,
    },
    ErrorFrame {
        session_id: String,
        error: String,
        recoverable: bool,
    },
    UserStartedSpeakingFrame {
        session_id: String,
    },
    UserStoppedSpeakingFrame {
        session_id: String,
    },
    CancelFrame {
        session_id: String,
    },
}

impl SystemFrame {
    pub fn session_id(&self) -> &str {
        match self {
            SystemFrame::StartFrame { session_id, .. } => session_id,
            SystemFrame::EndFrame { session_id, .. } => session_id,
            SystemFrame::StartInterruptionFrame { session_id, .. } => session_id,
            SystemFrame::StopInterruptionFrame { session_id } => session_id,
            SystemFrame::ErrorFrame { session_id, .. } => session_id,
            SystemFrame::UserStartedSpeakingFrame { session_id } => session_id,
            SystemFrame::UserStoppedSpeakingFrame { session_id } => session_id,
            SystemFrame::CancelFrame { session_id } => session_id,
        }
    }
}
