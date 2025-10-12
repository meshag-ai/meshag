 // services/common/src/interrupt.rs
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ControlMessage {
    Interrupt {
        session_id: String,
        by: String,        // "user" or "agent"
        reason: Option<String>,
        timestamp: Option<i64>, // unix epoch millis (optional)
    },
    // (future) Pause / Resume / ReplaceInput / PriorityChange etc.
}

// event for publishing state updates to clients/observability
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: String, // e.g., "interrupted", "generation_cancelled"
    pub detail: Option<String>,
    pub timestamp: Option<i64>,
}
