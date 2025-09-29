use anyhow::Result;
use meshag_connectors::{ElevenLabs, ElevenLabsConfig};
use meshag_service_common::server;
use meshag_shared::EventQueue;
use std::sync::Arc;
use tracing::info;
use tts_service::{TtsService, TtsServiceState};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting TTS Service");

    let event_queue = EventQueue::new("tts-service").await?;

    let mut tts_service = TtsService::new();

    if let Ok(api_key) = std::env::var("ELEVENLABS_API_KEY") {
        let config = ElevenLabsConfig::new(api_key);
        let elevenlabs_connector = ElevenLabs::tts_connector(config);
        tts_service
            .register_connector("elevenlabs", elevenlabs_connector)
            .await;
        info!("Registered ElevenLabs connector");
    }

    let tts_clone = tts_service.clone();
    let queue_clone = event_queue.clone();
    tokio::spawn(async move {
        if let Err(e) = tts_clone.start_processing(queue_clone).await {
            tracing::error!("TTS processing failed: {}", e);
        }
    });

    let state = Arc::new(TtsServiceState {
        event_queue,
        tts_service,
    });

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8083".to_string())
        .parse()?;

    info!("TTS Service listening on port {}", port);
    server::run(state, port).await?;

    Ok(())
}
