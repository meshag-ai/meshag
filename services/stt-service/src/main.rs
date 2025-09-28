use anyhow::Result;
use meshag_connectors::{Deepgram, DeepgramConfig, OpenAI, OpenAIConfig};
use meshag_service_common::server;
use meshag_shared::EventQueue;
use std::sync::Arc;
use stt_service::{SttService, SttServiceState};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting STT Service");

    // Initialize NATS connection
    let event_queue = EventQueue::new("stt-service").await?;

    // Initialize connectors based on environment configuration
    let mut stt_service = SttService::new();

    if let Ok(api_key) = std::env::var("DEEPGRAM_API_KEY") {
        let deepgram = Deepgram::stt_connector(DeepgramConfig::new(api_key));
        stt_service.register_connector("deepgram", deepgram).await;
        info!("Registered Deepgram connector");
    }

    // Start the STT processing loop in the background
    let stt_clone = stt_service.clone();
    let queue_clone = event_queue.clone();
    tokio::spawn(async move {
        if let Err(e) = stt_clone.start_processing(queue_clone).await {
            tracing::error!("STT processing failed: {}", e);
        }
    });

    // Create service state for HTTP handlers
    let state = Arc::new(SttServiceState {
        event_queue,
        stt_service,
    });

    // Start HTTP server for health checks and metrics
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse()?;

    info!("STT Service listening on port {}", port);
    server::run(state, port).await?;

    Ok(())
}
