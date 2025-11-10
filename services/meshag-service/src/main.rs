use anyhow::Result;
use tracing::info;

mod gateway;
mod llm;
mod stt;
mod transport;
mod tts;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let service_type = std::env::var("SERVICE_TYPE").unwrap_or_else(|_| "gateway".to_string());

    info!("Starting Meshag service with type: {}", service_type);

    match service_type.as_str() {
        "gateway" | "api-gateway" => gateway::run_gateway_service().await?,
        "llm" => llm::run_llm_service().await?,
        "stt" => stt::run_stt_service().await?,
        "tts" => tts::run_tts_service().await?,
        "transport" => transport::run_transport_service().await?,
        _ => panic!(
            "Unknown SERVICE_TYPE: {}. Valid options: gateway, llm, stt, tts, transport",
            service_type
        ),
    }

    Ok(())
}
