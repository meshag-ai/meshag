use anyhow::Result;
use meshag_connectors::{OpenAI, OpenAIConfig};
use meshag_service_common::server;
use meshag_services_llm::{LlmService, LlmServiceState, MultiSessionLlmService};
use meshag_shared::{EventQueue, StreamConfig};
use std::sync::Arc;
use tracing::info;

pub async fn run_llm_service() -> Result<()> {
    info!("Starting LLM Service with multi-session support");

    let event_queue = EventQueue::new("llm-service").await?;

    event_queue.ensure_stream(StreamConfig::system_stream()).await?;
    event_queue.ensure_stream(StreamConfig::data_stream()).await?;

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

    let multi_session = MultiSessionLlmService::new(llm_service.clone());

    let queue_clone = event_queue.clone();
    tokio::spawn(async move {
        if let Err(e) = multi_session.start_dispatcher(queue_clone).await {
            tracing::error!("Multi-session LLM dispatcher failed: {}", e);
        }
    });

    let state = Arc::new(LlmServiceState {
        event_queue,
        llm_service,
    });

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8082".to_string())
        .parse()?;

    info!("LLM Service listening on port {}", port);
    server::run(state, port).await?;

    Ok(())
}
