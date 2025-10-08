use anyhow::Result;
use async_nats::jetstream::{self, consumer::PullConsumer, stream::Stream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingEvent {
    pub session_id: String,
    pub conversation_id: Uuid,
    pub correlation_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp_ms: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TwilioMediaData {
    pub track: String,
    pub chunk: String,
    pub timestamp: String,
    pub payload: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TwilioStartData {
    pub account_sid: String,
    pub call_sid: String,
    pub stream_sid: String,
    pub tracks: Vec<String>,
    pub media_format: TwilioMediaFormat,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TwilioMediaFormat {
    pub encoding: String,
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
    pub channels: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEventPayload {
    pub call_sid: String,
    pub stream_sid: String,
    pub track: String,
    pub chunk: String,
    pub timestamp: String,
    pub payload: String, // Base64 encoded audio data
    pub media_format: TwilioMediaFormat,
}

pub struct StreamConfig {
    pub name: String,
    pub subjects: Vec<String>,
    pub max_messages: i64,
    pub max_bytes: i64,
    pub max_age: std::time::Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StreamType {
    STTStream,
    LLMStream,
    TTSStream,
}

impl StreamType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StreamType::STTStream => "STT_STREAM",
            StreamType::LLMStream => "LLM_STREAM",
            StreamType::TTSStream => "TTS_STREAM",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SubjectName {
    STTSubject,
    LLMSubject,
    TTSSubject,
    TransportSubject,
}

impl SubjectName {
    pub fn as_str(&self, session_id: Option<String>) -> &'static str {
        let subject = match self {
            SubjectName::STTSubject => "STT_SUBJECT.session.{session_id}",
            SubjectName::LLMSubject => "LLM_SUBJECT.session.{session_id}",
            SubjectName::TTSSubject => "TTS_SUBJECT.session.{session_id}",
            SubjectName::TransportSubject => "TRANSPORT_SUBJECT.session.{session_id}",
        };

        match session_id {
            Some(session_id) => {
                let subject_str = subject.replace("{session_id}", &session_id);
                Box::leak(subject_str.into_boxed_str())
            }
            None => {
                let subject_str = subject.replace("{session_id}", "*");
                Box::leak(subject_str.into_boxed_str())
            }
        }
    }
}

impl StreamConfig {
    #[must_use]
    pub fn stt_stream() -> Self {
        Self {
            name: StreamType::STTStream.as_str().to_string(),
            subjects: vec![SubjectName::STTSubject.as_str(None).to_string()],
            max_messages: 100_000,
            max_bytes: 100_000_000,                       // 100MB
            max_age: std::time::Duration::from_secs(300), // 5 minutes
        }
    }

    #[must_use]
    pub fn llm_stream() -> Self {
        Self {
            name: StreamType::LLMStream.as_str().to_string(),
            subjects: vec![SubjectName::LLMSubject.as_str(None).to_string()],
            max_messages: 100_000,
            max_bytes: 10_000_000, // 10MB (text is smaller)
            max_age: std::time::Duration::from_secs(600), // 10 minutes
        }
    }

    #[must_use]
    pub fn tts_stream() -> Self {
        Self {
            name: StreamType::TTSStream.as_str().to_string(),
            subjects: vec![SubjectName::TTSSubject.as_str(None).to_string()],
            max_messages: 100_000,
            max_bytes: 50_000_000,                        // 50MB
            max_age: std::time::Duration::from_secs(600), // 10 minutes
        }
    }
}

#[derive(Clone)]
pub struct EventQueue {
    client: async_nats::Client,
    jetstream: jetstream::Context,
    consumer_name: String,
}

impl EventQueue {
    pub async fn new(consumer_name: &str) -> Result<Self, anyhow::Error> {
        let nats_url =
            std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

        info!("Connecting to NATS at {}", nats_url);

        let client = async_nats::connect(&nats_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to NATS: {}", e))?;
        let jetstream = jetstream::new(client.clone());

        Ok(Self {
            client,
            jetstream,
            consumer_name: consumer_name.to_string(),
        })
    }

    pub async fn ensure_stream(&self, config: StreamConfig) -> Result<Stream, anyhow::Error> {
        let stream_config = jetstream::stream::Config {
            name: config.name.clone(),
            subjects: config.subjects,
            max_messages: config.max_messages,
            max_bytes: config.max_bytes,
            max_age: config.max_age,
            storage: jetstream::stream::StorageType::Memory,
            num_replicas: 1,
            ..Default::default()
        };

        match self.jetstream.create_stream(stream_config).await {
            Ok(stream) => {
                info!("Stream '{}' created successfully", config.name);
                Ok(stream)
            }
            Err(e) => match self.jetstream.get_stream(&config.name).await {
                Ok(stream) => {
                    info!("Using existing stream '{}'", config.name);
                    Ok(stream)
                }
                Err(_) => {
                    error!("Failed to create or get stream '{}': {e}", config.name);
                    Err(anyhow::anyhow!("Stream creation failed: {e}"))
                }
            },
        }
    }

    pub async fn publish_event(&self, subject: &str, event: ProcessingEvent) -> Result<String> {
        let payload = serde_json::to_vec(&event)?;

        let ack = self
            .jetstream
            .publish(subject.to_string(), payload.into())
            .await?
            .await?;

        let message_id = format!("{}:{}", ack.stream, ack.sequence);

        Ok(message_id)
    }

    pub async fn create_consumer(
        &self,
        stream_config: StreamConfig,
        subject: String,
    ) -> Result<PullConsumer> {
        let stream = self.ensure_stream(stream_config).await?;

        let consumer_config = jetstream::consumer::pull::Config {
            durable_name: Some(self.consumer_name.clone()),
            description: Some(format!("Consumer for {subject} processing")),
            deliver_policy: jetstream::consumer::DeliverPolicy::New,
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ack_wait: std::time::Duration::from_secs(30),
            max_deliver: 3,
            filter_subject: subject.clone(),
            ..Default::default()
        };

        let consumer = stream.create_consumer(consumer_config).await?;

        info!(
            subject = %subject,
            consumer = %self.consumer_name,
            "Created NATS JetStream consumer"
        );

        Ok(consumer)
    }

    pub async fn consume_events<F, Fut>(
        &self,
        stream_config: StreamConfig,
        subject: String,
        callback: F,
    ) -> Result<()>
    where
        F: Fn(ProcessingEvent) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let consumer = self.create_consumer(stream_config, subject.clone()).await?;

        info!(
            subject = %subject,
            consumer = %self.consumer_name,
            "Starting NATS JetStream event consumer"
        );

        loop {
            match consumer.messages().await {
                Ok(mut messages) => {
                    while let Some(message) = messages.next().await {
                        match message {
                            Ok(msg) => {
                                match serde_json::from_slice::<ProcessingEvent>(&msg.payload) {
                                    Ok(event) => match callback(event.clone()).await {
                                        Ok(()) => {
                                            if let Err(e) = msg.ack().await {
                                                error!("Failed to ack message: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            error!(
                                                correlation_id = %event.correlation_id,
                                                error = %e,
                                                "Failed to process event, will retry"
                                            );
                                            if let Err(nak_err) = msg
                                                .ack_with(async_nats::jetstream::AckKind::Nak(None))
                                                .await
                                            {
                                                error!("Failed to nak message: {}", nak_err);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        error!("Failed to parse event payload: {}", e);
                                        let _ = msg.ack().await;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error receiving message: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error creating message stream: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }
            }
        }
    }

    pub async fn publish_nats_core(&self, subject: &str, event: ProcessingEvent) -> Result<()> {
        let payload = serde_json::to_vec(&event)?;
        self.client
            .publish(subject.to_string(), payload.into())
            .await?;
        Ok(())
    }

    pub async fn subscribe_nats_core<F, Fut>(&self, subject: String, callback: F) -> Result<()>
    where
        F: Fn(ProcessingEvent) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let mut subscriber = self.client.subscribe(subject.clone()).await?;

        info!(
            subject = %subject,
            "Starting NATS Core subscriber"
        );

        while let Some(message) = subscriber.next().await {
            match serde_json::from_slice::<ProcessingEvent>(&message.payload) {
                Ok(event) => {
                    if let Err(e) = callback(event.clone()).await {
                        error!(
                            session_id = %event.session_id,
                            error = %e,
                            "Failed to process NATS Core event"
                        );
                    }
                }
                Err(e) => {
                    error!("Failed to parse NATS Core event payload: {}", e);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetrics {
    pub stream_name: String,
    pub total_messages: u64,
    pub pending_messages: u64,
    pub delivered_messages: u64,
    pub consumer_lag: u64,
    pub bytes_stored: u64,
}
