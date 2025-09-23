use anyhow::Result;
use std::collections::HashMap;
use uuid::Uuid;

pub mod config;
pub mod router;
pub mod valkey_config;

pub use config::*;
pub use router::*;
pub use valkey_config::*;

/// Main orchestrator for the distributed AI agent system
#[derive(Debug, Clone)]
pub struct AgentOrchestrator {
    pub config: AgentConfig,
    pub session_id: Uuid,
}

impl AgentOrchestrator {
    /// Create a new orchestrator from a configuration file
    pub async fn from_config_file(config_path: &str) -> Result<Self> {
        let config = AgentConfig::from_file(config_path).await?;

        Ok(Self {
            config,
            session_id: Uuid::new_v4(),
        })
    }

    /// Create a new orchestrator from a JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let config = AgentConfig::from_json(json)?;

        Ok(Self {
            config,
            session_id: Uuid::new_v4(),
        })
    }

    /// Get the system prompt
    pub fn get_system_prompt(&self) -> &str {
        &self.config.system_prompt
    }

    /// Get pipeline configuration
    pub fn get_pipeline(&self) -> &PipelineConfig {
        &self.config.pipeline
    }

    /// Get connector configuration for a service
    pub fn get_connector_config(&self, service_name: &str) -> Option<&ConnectorConfig> {
        self.config.get_connector_config(service_name)
    }

    /// Get the next service in the pipeline for a given service
    pub fn get_next_service(&self, current_service: &str) -> Option<(&str, &str)> {
        self.config
            .get_next_service(current_service)
            .map(|step| (step.to.as_str(), step.stream.as_str()))
    }

    /// Get the previous service in the pipeline for a given service
    pub fn get_previous_service(&self, current_service: &str) -> Option<(&str, &str)> {
        self.config
            .get_previous_service(current_service)
            .map(|step| (step.from.as_str(), step.stream.as_str()))
    }

    /// Validate configuration
    pub fn validate_config(&self) -> Result<()> {
        self.config.validate()
    }

    /// Get all services in the pipeline
    pub fn get_pipeline_services(&self) -> Vec<&str> {
        let mut services = Vec::new();

        // Add all 'from' services
        for step in &self.config.pipeline.flow {
            if !services.contains(&step.from.as_str()) {
                services.push(step.from.as_str());
            }
        }

        // Add all 'to' services
        for step in &self.config.pipeline.flow {
            if !services.contains(&step.to.as_str()) {
                services.push(step.to.as_str());
            }
        }

        services
    }

    /// Get pipeline flow as a list of steps
    pub fn get_pipeline_flow(&self) -> &[FlowStep] {
        &self.config.pipeline.flow
    }

    /// Check if a service is the first in the pipeline
    pub fn is_first_service(&self, service_name: &str) -> bool {
        self.config.is_first_service(service_name)
    }

    /// Check if a service is the last in the pipeline
    pub fn is_last_service(&self, service_name: &str) -> bool {
        self.config.is_last_service(service_name)
    }

    /// Get all services that consume from a given service
    pub fn get_consumers(&self, service_name: &str) -> Vec<(&str, &str)> {
        self.config
            .get_consumers(service_name)
            .into_iter()
            .map(|step| (step.to.as_str(), step.stream.as_str()))
            .collect()
    }

    /// Get all services that produce for a given service
    pub fn get_producers(&self, service_name: &str) -> Vec<(&str, &str)> {
        self.config
            .get_producers(service_name)
            .into_iter()
            .map(|step| (step.from.as_str(), step.stream.as_str()))
            .collect()
    }

    /// Get pipeline summary
    pub fn get_pipeline_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut summary = HashMap::new();

        summary.insert(
            "name".to_string(),
            serde_json::Value::String(self.config.pipeline.name.clone()),
        );

        summary.insert(
            "flow_steps".to_string(),
            serde_json::Value::Number(self.config.pipeline.flow.len().into()),
        );

        summary.insert(
            "services".to_string(),
            serde_json::Value::Array(
                self.get_pipeline_services()
                    .into_iter()
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .collect(),
            ),
        );

        summary.insert(
            "connectors".to_string(),
            serde_json::Value::Array(
                self.config
                    .connectors
                    .keys()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
        );

        summary
    }
}
