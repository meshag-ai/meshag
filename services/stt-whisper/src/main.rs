use anyhow::Result;
use meshag_service_common::server;
use meshag_service_common::{handlers::ServiceState, types::HealthCheck};
use meshag_shared::EventQueue;
use meshag_stt_common::session::SessionManager;
use std::sync::Arc;
use tracing::Level;

mod processor;

/// The state for our STT service
pub struct SttServiceState {
    pub event_queue: EventQueue,
    pub session_manager: SessionManager,
}

#[async_trait::async_trait]
impl ServiceState for SttServiceState {
    fn service_name(&self) -> String {
        "stt-whisper".to_string()
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }

    async fn is_ready(&self) -> Vec<HealthCheck> {
        // Add any service-specific readiness checks here.
        // For now, we just have the in-memory session manager which is always "ready".
        vec![HealthCheck {
            name: "session_manager".to_string(),
            status: "healthy".to_string(),
            message: None,
        }]
    }

    async fn get_metrics(&self) -> Vec<String> {
        let active_sessions = self
            .session_manager
            .get_active_sessions_count()
            .await
            .unwrap_or(0);
        vec![
            format!("# HELP stt_active_sessions Active audio buffering sessions"),
            format!("# TYPE stt_active_sessions gauge"),
            format!("stt_active_sessions {}", active_sessions),
        ]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let event_queue = EventQueue::new("stt-whisper-service").await?;

    // Create the application state
    let state = Arc::new(SttServiceState {
        event_queue: event_queue.clone(),
        session_manager: session_manager.clone(),
    });

    // Start the specific STT processor logic in the background
    let whisper_processor = processor::WhisperProcessor::new()?;
    tokio::spawn(async move {
        if let Err(e) =
            meshag_stt_common::run_processor(whisper_processor, session_manager, event_queue).await
        {
            tracing::error!("STT processor failed: {}", e);
        }
    });

    // Start the generic HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse()?;
    server::run(state, port).await?;

    Ok(())
}
