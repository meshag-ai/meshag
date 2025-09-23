use anyhow::Result;
use chrono::{DateTime, Utc};
use meshag_shared::{EventQueue, ProcessingEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::valkey_config::ConfigManager;

/// Event routing system for services
#[derive(Clone)]
pub struct ServiceRouter {
    config_manager: ConfigManager,
    event_queue: EventQueue,
    service_name: String,
}

/// Routing event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub source_service: String,
    pub target_service: String,
    pub stream: String,
    pub payload: serde_json::Value,
    pub session_id: String,
    pub context: HashMap<String, String>,
}

impl RoutingEvent {
    pub fn new(
        source_service: String,
        target_service: String,
        stream: String,
        payload: serde_json::Value,
        session_id: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source_service,
            target_service,
            stream,
            payload,
            session_id,
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.context.insert(key, value);
        self
    }
}

impl ServiceRouter {
    /// Create a new service router
    pub async fn new(
        valkey_url: &str,
        event_queue: EventQueue,
        service_name: String,
    ) -> Result<Self> {
        let config_manager = ConfigManager::new(valkey_url).await?;

        Ok(Self {
            config_manager,
            event_queue,
            service_name,
        })
    }

    /// Load configuration for a session
    pub async fn load_config(&mut self, session_id: &str) -> Result<bool> {
        self.config_manager.load_config(session_id).await
    }

    /// Route output to the next service in the pipeline
    pub async fn route_to_next(&self, payload: serde_json::Value, session_id: &str) -> Result<()> {
        if let Some((next_service, stream)) =
            self.config_manager.get_next_service(&self.service_name)
        {
            let event = RoutingEvent::new(
                self.service_name.clone(),
                next_service.to_string(),
                stream.to_string(),
                payload,
                session_id.to_string(),
            );

            self.send_event(event).await?;

            tracing::info!(
                "Routed from {} to {} via stream {}",
                self.service_name,
                next_service,
                stream
            );
        } else {
            tracing::warn!(
                "No next service found for {} in pipeline",
                self.service_name
            );
        }

        Ok(())
    }

    /// Route input from the previous service in the pipeline
    pub async fn route_from_previous(
        &self,
        payload: serde_json::Value,
        session_id: &str,
    ) -> Result<()> {
        if let Some((prev_service, stream)) =
            self.config_manager.get_previous_service(&self.service_name)
        {
            let event = RoutingEvent::new(
                prev_service.to_string(),
                self.service_name.clone(),
                stream.to_string(),
                payload,
                session_id.to_string(),
            );

            self.send_event(event).await?;

            tracing::info!(
                "Routed from {} to {} via stream {}",
                prev_service,
                self.service_name,
                stream
            );
        } else {
            tracing::warn!(
                "No previous service found for {} in pipeline",
                self.service_name
            );
        }

        Ok(())
    }

    /// Send an event to a specific service
    pub async fn send_to_service(
        &self,
        target_service: &str,
        stream: &str,
        payload: serde_json::Value,
        session_id: &str,
    ) -> Result<()> {
        let event = RoutingEvent::new(
            self.service_name.clone(),
            target_service.to_string(),
            stream.to_string(),
            payload,
            session_id.to_string(),
        );

        self.send_event(event).await?;
        Ok(())
    }

    /// Send event via NATS
    async fn send_event(&self, event: RoutingEvent) -> Result<()> {
        let processing_event = ProcessingEvent {
            conversation_id: Uuid::parse_str(&event.session_id).unwrap_or_else(|_| Uuid::new_v4()),
            correlation_id: event.id,
            event_type: event.stream.clone(),
            payload: event.payload,
            timestamp_ms: event.timestamp.timestamp_millis() as u64,
            source_service: event.source_service.clone(),
            target_service: event.target_service.clone(),
        };

        // Create subject based on target service and stream
        let subject = format!("{}.{}", event.target_service, event.stream);

        self.event_queue
            .publish_event(&subject, processing_event)
            .await?;

        tracing::debug!("Sent event {} to subject {}", event.id, subject);

        Ok(())
    }

    /// Get connector configuration for this service
    pub fn get_connector_config(&self) -> Option<&crate::config::ConnectorConfig> {
        self.config_manager.get_connector_config(&self.service_name)
    }

    /// Get system prompt
    pub fn get_system_prompt(&self) -> Option<&str> {
        self.config_manager.get_system_prompt()
    }

    /// Check if this service is the first in the pipeline
    pub fn is_first_service(&self) -> bool {
        self.config_manager
            .get_config()
            .map(|config| config.is_first_service(&self.service_name))
            .unwrap_or(false)
    }

    /// Check if this service is the last in the pipeline
    pub fn is_last_service(&self) -> bool {
        self.config_manager
            .get_config()
            .map(|config| config.is_last_service(&self.service_name))
            .unwrap_or(false)
    }

    /// Get current session ID
    pub fn get_session_id(&self) -> Option<&str> {
        self.config_manager.get_session_id()
    }

    /// Check if configuration is loaded
    pub fn is_config_loaded(&self) -> bool {
        self.config_manager.is_loaded()
    }

    /// Get routing statistics
    pub fn get_routing_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();

        stats.insert(
            "service_name".to_string(),
            serde_json::Value::String(self.service_name.clone()),
        );

        stats.insert(
            "config_loaded".to_string(),
            serde_json::Value::Bool(self.is_config_loaded()),
        );

        if let Some(session_id) = self.get_session_id() {
            stats.insert(
                "session_id".to_string(),
                serde_json::Value::String(session_id.to_string()),
            );
        }

        stats.insert(
            "is_first_service".to_string(),
            serde_json::Value::Bool(self.is_first_service()),
        );

        stats.insert(
            "is_last_service".to_string(),
            serde_json::Value::Bool(self.is_last_service()),
        );

        if let Some(config) = self.config_manager.get_config() {
            stats.insert(
                "pipeline_name".to_string(),
                serde_json::Value::String(config.pipeline.name.clone()),
            );

            stats.insert(
                "flow_steps".to_string(),
                serde_json::Value::Number(config.pipeline.flow.len().into()),
            );
        }

        stats
    }
}
