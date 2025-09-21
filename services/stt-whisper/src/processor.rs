//! STT processor using the Whisper connector

use anyhow::Result;
use meshag_connectors::stt::{whisper::WhisperConnector, SttConnector};

/// STT processor implementation using WhisperConnector
#[derive(Clone)]
pub struct WhisperProcessor {
    connector: WhisperConnector,
}

impl WhisperProcessor {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
        let base_url = std::env::var("OPENAI_BASE_URL").ok();

        let connector = WhisperConnector::new(api_key, base_url);

        Ok(Self { connector })
    }

    pub fn connector(&self) -> &WhisperConnector {
        &self.connector
    }
}
