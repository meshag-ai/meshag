//! Ultra-high-performance queue operations using NATS JetStream
//!
//! This module provides event streaming capabilities for the distributed Meshag architecture.
//! Each processor consumes from one stream and publishes to the next stream.
//! ```rust
//! use meshag_shared::queue::EventQueue;
//!
//! let event_queue = EventQueue::new("test_consumer").await?;
//! event_queue.publish_event("test_subject", "test_event").await?;
//!
//! let event = event_queue.consume_events("test_stream", "test_subject", |event| {
//!     println!("Received event: {}", event);
//!     Ok(())
//! }).await?;
//!
//! println!("Received event: {}", event);
//!
//! let stream_info = event_queue.get_stream_info("test_stream").await?;
//! println!("Stream info: {:?}", stream_info);
//!
//! let queue_depth = event_queue.get_queue_depth("test_stream").await?;
//! println!("Queue depth: {}", queue_depth);
//!
//! let metrics = event_queue.get_metrics("test_stream").await?;
//! println!("Metrics: {:?}", metrics);
//!
//! let health_check = event_queue.health_check().await?;
//! println!("Health check: {:?}", health_check);
//! ```

use anyhow::Result;
use async_nats::jetstream::{self, consumer::PullConsumer, stream::Stream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

/// Event published between processors in the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingEvent {
    pub conversation_id: Uuid,
    pub correlation_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp_ms: u64,
    pub source_service: String,
    pub target_service: String,
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

/// Media event payload for NATS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEventPayload {
    pub session_id: String,
    pub call_sid: String,
    pub stream_sid: String,
    pub track: String,
    pub chunk: String,
    pub timestamp: String,
    pub payload: String, // Base64 encoded audio data
    pub media_format: TwilioMediaFormat,
}

/// Stream configuration for different event types
pub struct StreamConfig {
    pub name: String,
    pub subjects: Vec<String>,
    pub max_messages: i64,
    pub max_bytes: i64,
    pub max_age: std::time::Duration,
}

impl StreamConfig {
    /// Audio input stream configuration
    pub fn audio_input() -> Self {
        Self {
            name: "AUDIO_INPUT".to_string(),
            subjects: vec!["AUDIO_INPUT".to_string()],
            max_messages: 100_000,
            max_bytes: 100_000_000,                       // 100MB
            max_age: std::time::Duration::from_secs(300), // 5 minutes
        }
    }

    /// STT output stream configuration
    pub fn stt_output() -> Self {
        Self {
            name: "STT_OUTPUT".to_string(),
            subjects: vec!["STT_OUTPUT".to_string()],
            max_messages: 100_000,
            max_bytes: 10_000_000, // 10MB (text is smaller)
            max_age: std::time::Duration::from_secs(600), // 10 minutes
        }
    }

    /// LLM output stream configuration
    pub fn llm_output() -> Self {
        Self {
            name: "LLM_OUTPUT".to_string(),
            subjects: vec!["LLM_OUTPUT".to_string()],
            max_messages: 100_000,
            max_bytes: 50_000_000,                        // 50MB
            max_age: std::time::Duration::from_secs(600), // 10 minutes
        }
    }

    /// TTS output stream configuration
    pub fn tts_output() -> Self {
        Self {
            name: "TTS_OUTPUT".to_string(),
            subjects: vec!["TTS_OUTPUT".to_string()],
            max_messages: 50_000,
            max_bytes: 500_000_000, // 500MB (audio is large)
            max_age: std::time::Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Ultra-high-performance queue manager using NATS JetStream
#[derive(Clone)]
pub struct EventQueue {
    client: async_nats::Client,
    jetstream: jetstream::Context,
    consumer_name: String,
}

impl EventQueue {
    /// Create new EventQueue with NATS JetStream connection
    pub async fn new(consumer_name: &str) -> Result<Self> {
        let nats_url =
            std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

        info!("Connecting to NATS at {}", nats_url);

        let client = async_nats::connect(&nats_url).await?;
        let jetstream = jetstream::new(client.clone());

        Ok(Self {
            client,
            jetstream,
            consumer_name: consumer_name.to_string(),
        })
    }

    /// Ensure stream exists with proper configuration
    pub async fn ensure_stream(&self, config: StreamConfig) -> Result<Stream> {
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
            Err(e) => {
                // Try to get existing stream if creation failed
                match self.jetstream.get_stream(&config.name).await {
                    Ok(stream) => {
                        info!("Using existing stream '{}'", config.name);
                        Ok(stream)
                    }
                    Err(_) => {
                        error!("Failed to create or get stream '{}': {}", config.name, e);
                        Err(anyhow::anyhow!("Stream creation failed: {}", e))
                    }
                }
            }
        }
    }

    /// Publish event to NATS JetStream
    pub async fn publish_event(&self, subject: &str, event: ProcessingEvent) -> Result<String> {
        let payload = serde_json::to_vec(&event)?;

        let ack = self
            .jetstream
            .publish(subject.to_string(), payload.into())
            .await?
            .await?;

        let message_id = format!("{}:{}", ack.stream, ack.sequence);

        // info!(
        //     subject = subject,
        //     message_id = %message_id,
        //     conversation_id = %event.conversation_id,
        //     correlation_id = %event.correlation_id,
        //     "Published event to NATS JetStream"
        // );

        Ok(message_id)
    }

    /// Create consumer for JetStream with proper API
    pub async fn create_consumer(
        &self,
        stream_config: StreamConfig,
        subject: String,
    ) -> Result<PullConsumer> {
        // Ensure stream exists first
        let stream = self.ensure_stream(stream_config).await?;

        let consumer_config = jetstream::consumer::pull::Config {
            durable_name: Some(format!(
                "{}_{}",
                subject
                    .replace('.', "_")
                    .replace('/', "_")
                    .replace('\\', "_"),
                self.consumer_name
                    .replace('.', "_")
                    .replace('/', "_")
                    .replace('\\', "_")
            )),
            description: Some(format!("Consumer for {} processing", subject)),
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

    /// Consume events from NATS JetStream with proper error handling
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
            // Use the correct API for fetching messages (0.32 version)
            match consumer.messages().await {
                Ok(mut messages) => {
                    // Process messages in batches for high performance
                    while let Some(message) = messages.next().await {
                        match message {
                            Ok(msg) => {
                                match serde_json::from_slice::<ProcessingEvent>(&msg.payload) {
                                    Ok(event) => {
                                        match callback(event.clone()).await {
                                            Ok(_) => {
                                                // Acknowledge successful processing
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
                                                // NAK the message for retry (using ack_with)
                                                if let Err(nak_err) = msg
                                                    .ack_with(async_nats::jetstream::AckKind::Nak(
                                                        None,
                                                    ))
                                                    .await
                                                {
                                                    error!("Failed to nak message: {}", nak_err);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse event payload: {}", e);
                                        // Acknowledge malformed messages to prevent redelivery
                                        let _ = msg.ack().await;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error receiving message: {}", e);
                                break; // Break inner loop, continue outer loop
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

    /// Health check for NATS connection
    pub async fn health_check(&self) -> Result<bool> {
        match self.client.connection_state() {
            async_nats::connection::State::Connected => Ok(true),
            _ => Ok(false),
        }
    }

    /// Get stream info for monitoring
    pub async fn get_stream_info(&self, stream_name: &str) -> Result<jetstream::stream::Info> {
        let mut stream = self.jetstream.get_stream(stream_name).await?;
        let info = stream.info().await?;
        Ok(info.clone())
    }

    /// Get queue depth (pending messages) for monitoring
    pub async fn get_queue_depth(&self, stream_name: &str) -> Result<u64> {
        match self.get_stream_info(stream_name).await {
            Ok(info) => Ok(info.state.messages),
            Err(_) => Ok(0),
        }
    }

    /// Get JetStream metrics for monitoring
    pub async fn get_metrics(&self, stream_name: &str) -> Result<StreamMetrics> {
        match self.get_stream_info(stream_name).await {
            Ok(stream_info) => {
                Ok(StreamMetrics {
                    stream_name: stream_name.to_string(),
                    total_messages: stream_info.state.messages,
                    pending_messages: 0, // Would need consumer-specific info
                    delivered_messages: stream_info.state.last_sequence,
                    consumer_lag: 0, // Would need consumer-specific info
                    bytes_stored: stream_info.state.bytes,
                })
            }
            Err(_) => {
                // Return empty metrics if stream doesn't exist
                Ok(StreamMetrics {
                    stream_name: stream_name.to_string(),
                    total_messages: 0,
                    pending_messages: 0,
                    delivered_messages: 0,
                    consumer_lag: 0,
                    bytes_stored: 0,
                })
            }
        }
    }
}

/// Stream metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetrics {
    pub stream_name: String,
    pub total_messages: u64,
    pub pending_messages: u64,
    pub delivered_messages: u64,
    pub consumer_lag: u64,
    pub bytes_stored: u64,
}
