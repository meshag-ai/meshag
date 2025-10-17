use anyhow::Result;
use tracing::info;

mod llm;
mod stt;
mod tts;
mod transport;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let service_type = std::env::var("SERVICE_TYPE")
        .unwrap_or_else(|_| "llm".to_string());
    
    info!("Starting Meshag service with type: {}", service_type);
    
    match service_type.as_str() {
        "llm" => llm::run_llm_service().await?,
        "stt" => stt::run_stt_service().await?,
        "tts" => tts::run_tts_service().await?,
        "transport" => transport::run_transport_service().await?,
        _ => panic!("Unknown SERVICE_TYPE: {}. Valid options: llm, stt, tts, transport", service_type),
    }
    
    Ok(())
}
