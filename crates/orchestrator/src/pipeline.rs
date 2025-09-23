use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::config::{AgentConfig, PipelineConfig};

/// Pipeline execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineStatus {
    Initializing,
    Running,
    Paused,
    Stopped,
    Error(String),
}

/// Pipeline execution instance
#[derive(Debug, Clone)]
pub struct PipelineInstance {
    pub id: Uuid,
    pub name: String,
    pub config: PipelineConfig,
    pub status: PipelineStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub metrics: PipelineMetrics,
}

/// Pipeline execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMetrics {
    pub messages_processed: u64,
    pub total_latency_ms: u64,
    pub average_latency_ms: f64,
    pub error_count: u64,
    pub last_activity: Option<DateTime<Utc>>,
    pub throughput_per_minute: f64,
}

impl Default for PipelineMetrics {
    fn default() -> Self {
        Self {
            messages_processed: 0,
            total_latency_ms: 0,
            average_latency_ms: 0.0,
            error_count: 0,
            last_activity: None,
            throughput_per_minute: 0.0,
        }
    }
}

impl PipelineInstance {
    /// Create a new pipeline instance
    pub fn new(name: String, config: PipelineConfig, _agent_config: &AgentConfig) -> Result<Self> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        Ok(Self {
            id,
            name,
            config,
            status: PipelineStatus::Initializing,
            created_at: now,
            started_at: None,
            metrics: PipelineMetrics::default(),
        })
    }

    /// Start the pipeline
    pub async fn start(&mut self) -> Result<()> {
        self.status = PipelineStatus::Running;
        self.started_at = Some(Utc::now());

        tracing::info!("Pipeline '{}' started with ID: {}", self.name, self.id);
        Ok(())
    }

    /// Stop the pipeline
    pub async fn stop(&mut self) -> Result<()> {
        self.status = PipelineStatus::Stopped;
        tracing::info!("Pipeline '{}' stopped", self.name);
        Ok(())
    }

    /// Pause the pipeline
    pub async fn pause(&mut self) -> Result<()> {
        self.status = PipelineStatus::Paused;
        tracing::info!("Pipeline '{}' paused", self.name);
        Ok(())
    }

    /// Resume the pipeline
    pub async fn resume(&mut self) -> Result<()> {
        self.status = PipelineStatus::Running;
        tracing::info!("Pipeline '{}' resumed", self.name);
        Ok(())
    }

    /// Update pipeline metrics
    pub fn update_metrics(&mut self, latency_ms: u64) {
        self.metrics.messages_processed += 1;
        self.metrics.total_latency_ms += latency_ms;
        self.metrics.average_latency_ms =
            self.metrics.total_latency_ms as f64 / self.metrics.messages_processed as f64;
        self.metrics.last_activity = Some(Utc::now());

        // Calculate throughput (messages per minute)
        if let Some(started_at) = self.started_at {
            let duration_minutes = (Utc::now() - started_at).num_minutes() as f64;
            if duration_minutes > 0.0 {
                self.metrics.throughput_per_minute =
                    self.metrics.messages_processed as f64 / duration_minutes;
            }
        }
    }

    /// Record an error
    pub fn record_error(&mut self, error: &str) {
        self.metrics.error_count += 1;
        self.status = PipelineStatus::Error(error.to_string());
        tracing::error!("Pipeline '{}' error: {}", self.name, error);
    }

    /// Get pipeline health status
    pub fn health_status(&self) -> HashMap<String, serde_json::Value> {
        let mut status = HashMap::new();

        status.insert(
            "id".to_string(),
            serde_json::Value::String(self.id.to_string()),
        );
        status.insert(
            "name".to_string(),
            serde_json::Value::String(self.name.clone()),
        );
        status.insert(
            "status".to_string(),
            serde_json::Value::String(match &self.status {
                PipelineStatus::Initializing => "initializing".to_string(),
                PipelineStatus::Running => "running".to_string(),
                PipelineStatus::Paused => "paused".to_string(),
                PipelineStatus::Stopped => "stopped".to_string(),
                PipelineStatus::Error(e) => format!("error: {}", e),
            }),
        );
        status.insert(
            "created_at".to_string(),
            serde_json::Value::String(self.created_at.to_rfc3339()),
        );

        if let Some(started_at) = self.started_at {
            status.insert(
                "started_at".to_string(),
                serde_json::Value::String(started_at.to_rfc3339()),
            );
        }

        status.insert(
            "metrics".to_string(),
            serde_json::to_value(&self.metrics).unwrap(),
        );

        status
    }
}

/// Pipeline execution context
#[derive(Debug, Clone)]
pub struct PipelineContext {
    pub session_id: Uuid,
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl PipelineContext {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            user_id: None,
            conversation_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_conversation_id(mut self, conversation_id: String) -> Self {
        self.conversation_id = Some(conversation_id);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
