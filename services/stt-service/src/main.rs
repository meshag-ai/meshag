use anyhow::Result;
use meshag_connectors::{OpenAI, OpenAIConfig};
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

    // Add OpenAI Whisper connector if API key is available
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let mut config = OpenAIConfig::new(api_key);
        if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
            config = config.with_base_url(base_url);
        }
        let whisper_connector = OpenAI::stt_connector(config);
        stt_service
            .register_connector("openai", whisper_connector)
            .await;
        info!("Registered OpenAI Whisper connector");
    }

    // TODO: Add other connectors based on environment variables
    // if let Ok(api_key) = std::env::var("DEEPGRAM_API_KEY") {
    //     let deepgram = DeepgramConnector::new(api_key);
    //     stt_service.register_connector("deepgram", Box::new(deepgram));
    // }

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
