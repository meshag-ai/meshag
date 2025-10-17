use anyhow::Result;
use meshag_connectors::{OpenAI, OpenAIConfig};
use meshag_service_common::server;
use meshag_services_llm::{LlmService, LlmServiceState};
use meshag_shared::EventQueue;
use std::sync::Arc;
use tracing::info;

pub async fn run_llm_service() -> Result<()> {
    info!("Starting LLM Service");

    let event_queue = EventQueue::new("llm-service").await?;

    let mut llm_service = LlmService::new();

    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let mut config = OpenAIConfig::new(api_key);
        if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
            config = config.with_base_url(base_url);
        }
        let openai_connector = OpenAI::llm_connector(config);
        llm_service
            .register_connector("openai", openai_connector)
            .await;
        info!("Registered OpenAI connector");
    }

    let llm_clone = llm_service.clone();
    let queue_clone = event_queue.clone();
    tokio::spawn(async move {
        if let Err(e) = llm_clone.start_processing(queue_clone).await {
            tracing::error!("LLM processing failed: {}", e);
        }
    });

    // Create service state for HTTP handlers
    let state = Arc::new(LlmServiceState {
        event_queue,
        llm_service,
    });

    // Start HTTP server for health checks and metrics
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8082".to_string())
        .parse()?;

    info!("LLM Service listening on port {}", port);
    server::run(state, port).await?;

    Ok(())
}
