use anyhow::Result;
use dashmap::DashMap;
use meshag_shared::{EventQueue, StreamConfig};
use std::sync::Arc;
use uuid::Uuid;

pub struct SessionChannels {}

pub struct GatewayState {
    pub event_queue: EventQueue,
    pub sessions: Arc<DashMap<Uuid, SessionChannels>>,
}

impl GatewayState {
    pub async fn new() -> Result<Arc<Self>> {
        let event_queue = EventQueue::new("api-gateway").await?;

        event_queue
            .ensure_stream(StreamConfig::system_stream())
            .await?;
        event_queue
            .ensure_stream(StreamConfig::data_stream())
            .await?;

        Ok(Arc::new(Self {
            event_queue,
            sessions: Arc::new(DashMap::new()),
        }))
    }

    pub fn register_session(&self, session_id: Uuid) {
        self.sessions.insert(session_id, SessionChannels {});
    }

    pub fn unregister_session(&self, session_id: &Uuid) {
        self.sessions.remove(session_id);
    }
}
