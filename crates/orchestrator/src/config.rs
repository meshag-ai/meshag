use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simplified agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub pipeline: PipelineConfig,
    pub connectors: HashMap<String, ConnectorConfig>,
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub flow: Vec<FlowStep>,
}

/// Individual flow step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    pub from: String,
    pub to: String,
    pub stream: String,
}

/// Connector configuration for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub primary: String,
    pub fallback: Option<Vec<String>>,
    pub config: HashMap<String, serde_json::Value>,
}

impl AgentConfig {
    /// Load configuration from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let config: AgentConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from file
    pub async fn from_file(path: &str) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        Self::from_json(&content)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate pipeline flow
        for step in &self.pipeline.flow {
            if !self.connectors.contains_key(&step.from) {
                return Err(anyhow::anyhow!(
                    "Flow step references unknown service '{}'",
                    step.from
                ));
            }
            if !self.connectors.contains_key(&step.to) {
                return Err(anyhow::anyhow!(
                    "Flow step references unknown service '{}'",
                    step.to
                ));
            }
        }

        // Validate connectors have required fields
        for (service_name, connector) in &self.connectors {
            if connector.primary.is_empty() {
                return Err(anyhow::anyhow!(
                    "Service '{}' must have a primary connector",
                    service_name
                ));
            }
        }

        Ok(())
    }

    /// Get the next service in the pipeline for a given service
    pub fn get_next_service(&self, current_service: &str) -> Option<&FlowStep> {
        self.pipeline
            .flow
            .iter()
            .find(|step| step.from == current_service)
    }

    /// Get the previous service in the pipeline for a given service
    pub fn get_previous_service(&self, current_service: &str) -> Option<&FlowStep> {
        self.pipeline
            .flow
            .iter()
            .find(|step| step.to == current_service)
    }

    /// Get connector configuration for a service
    pub fn get_connector_config(&self, service_name: &str) -> Option<&ConnectorConfig> {
        self.connectors.get(service_name)
    }

    /// Get all services that consume from a given service
    pub fn get_consumers(&self, service_name: &str) -> Vec<&FlowStep> {
        self.pipeline
            .flow
            .iter()
            .filter(|step| step.from == service_name)
            .collect()
    }

    /// Get all services that produce for a given service
    pub fn get_producers(&self, service_name: &str) -> Vec<&FlowStep> {
        self.pipeline
            .flow
            .iter()
            .filter(|step| step.to == service_name)
            .collect()
    }

    /// Check if a service is the first in the pipeline
    pub fn is_first_service(&self, service_name: &str) -> bool {
        self.pipeline
            .flow
            .first()
            .map(|step| step.from == service_name)
            .unwrap_or(false)
    }

    /// Check if a service is the last in the pipeline
    pub fn is_last_service(&self, service_name: &str) -> bool {
        self.pipeline
            .flow
            .last()
            .map(|step| step.to == service_name)
            .unwrap_or(false)
    }
}
