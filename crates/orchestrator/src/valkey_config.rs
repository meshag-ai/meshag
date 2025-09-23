use anyhow::Result;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::AgentConfig;

/// Valkey configuration storage manager
#[derive(Debug, Clone)]
pub struct ConfigStorage {
    client: Client,
}

/// Configuration metadata stored in Valkey
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    pub session_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub ttl_seconds: u64,
}

impl ConfigStorage {
    /// Create a new config storage instance
    pub async fn new(valkey_url: &str) -> Result<Self> {
        let client = Client::open(valkey_url)?;

        // Test connection
        let mut conn = client.get_async_connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;

        tracing::info!("Connected to Valkey at: {}", valkey_url);
        Ok(Self { client })
    }

    /// Store configuration for a session
    pub async fn store_config(&self, session_id: &str, config: &AgentConfig) -> Result<()> {
        let key = format!("config:{}", session_id);

        let metadata = ConfigMetadata {
            session_id: session_id.to_string(),
            created_at: chrono::Utc::now(),
            version: "1.0".to_string(),
            ttl_seconds: 3600, // 1 hour
        };

        let config_data = ConfigData {
            metadata,
            config: config.clone(),
        };

        let payload = serde_json::to_string(&config_data)?;

        let mut conn = self.client.get_async_connection().await?;

        // Store with TTL
        let _: () = conn.set_ex(&key, payload, 3600).await?;

        // Also add to a set of all config keys for listing
        let _: () = conn.sadd("config:keys", &key).await?;

        tracing::info!("Stored configuration for session: {}", session_id);
        Ok(())
    }

    /// Retrieve configuration for a session
    pub async fn get_config(&self, session_id: &str) -> Result<Option<AgentConfig>> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;

        if let Ok(payload) = conn.get::<_, String>(&key).await {
            let config_data: ConfigData = serde_json::from_str(&payload)?;

            // Check if config is still valid (not expired)
            let now = chrono::Utc::now();
            let created_at = config_data.metadata.created_at;
            let ttl = Duration::from_secs(config_data.metadata.ttl_seconds);

            if now.signed_duration_since(created_at).to_std()? < ttl {
                tracing::info!("Retrieved configuration for session: {}", session_id);
                return Ok(Some(config_data.config));
            } else {
                tracing::warn!("Configuration for session {} has expired", session_id);
                // Clean up expired config
                self.delete_config(session_id).await?;
            }
        }

        Ok(None)
    }

    /// Delete configuration for a session
    pub async fn delete_config(&self, session_id: &str) -> Result<()> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;

        // Remove the config key
        let _: () = conn.del(&key).await?;

        // Remove from the set of config keys
        let _: () = conn.srem("config:keys", &key).await?;

        tracing::info!("Deleted configuration for session: {}", session_id);
        Ok(())
    }

    /// List all stored configurations
    pub async fn list_configs(&self) -> Result<Vec<String>> {
        let mut conn = self.client.get_async_connection().await?;

        // Get all config keys
        let keys: Vec<String> = conn.smembers("config:keys").await?;

        let mut session_ids = Vec::new();
        for key in keys {
            if let Some(session_id) = key.strip_prefix("config:") {
                session_ids.push(session_id.to_string());
            }
        }

        Ok(session_ids)
    }

    /// Check if configuration exists for a session
    pub async fn config_exists(&self, session_id: &str) -> Result<bool> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;
        let exists: bool = conn.exists(&key).await?;

        Ok(exists)
    }

    /// Get configuration metadata
    pub async fn get_config_metadata(&self, session_id: &str) -> Result<Option<ConfigMetadata>> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;

        if let Ok(payload) = conn.get::<_, String>(&key).await {
            let config_data: ConfigData = serde_json::from_str(&payload)?;
            return Ok(Some(config_data.metadata));
        }

        Ok(None)
    }

    /// Extend TTL for a configuration
    pub async fn extend_config_ttl(&self, session_id: &str, ttl_seconds: u64) -> Result<()> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;
        let _: () = conn.expire(&key, ttl_seconds as i64).await?;

        tracing::info!(
            "Extended TTL for session {} to {} seconds",
            session_id,
            ttl_seconds
        );
        Ok(())
    }

    /// Get TTL for a configuration
    pub async fn get_config_ttl(&self, session_id: &str) -> Result<Option<i64>> {
        let key = format!("config:{}", session_id);

        let mut conn = self.client.get_async_connection().await?;
        let ttl: i64 = conn.ttl(&key).await?;

        if ttl > 0 {
            Ok(Some(ttl))
        } else {
            Ok(None)
        }
    }

    /// Clean up expired configurations
    pub async fn cleanup_expired_configs(&self) -> Result<usize> {
        let mut conn = self.client.get_async_connection().await?;

        // Get all config keys
        let keys: Vec<String> = conn.smembers("config:keys").await?;

        let mut cleaned_count = 0;
        for key in keys {
            let ttl: i64 = conn.ttl(&key).await?;
            if ttl == -1 || ttl == -2 {
                // -1: no TTL, -2: key doesn't exist
                // Remove from the set
                let _: () = conn.srem("config:keys", &key).await?;
                cleaned_count += 1;
            }
        }

        if cleaned_count > 0 {
            tracing::info!("Cleaned up {} expired configurations", cleaned_count);
        }

        Ok(cleaned_count)
    }
}

/// Configuration data structure stored in Valkey
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigData {
    metadata: ConfigMetadata,
    config: AgentConfig,
}

/// Configuration manager for services
#[derive(Debug, Clone)]
pub struct ConfigManager {
    storage: ConfigStorage,
    current_config: Option<AgentConfig>,
    session_id: Option<String>,
}

impl ConfigManager {
    /// Create a new config manager
    pub async fn new(valkey_url: &str) -> Result<Self> {
        let storage = ConfigStorage::new(valkey_url).await?;

        Ok(Self {
            storage,
            current_config: None,
            session_id: None,
        })
    }

    /// Load configuration for a session
    pub async fn load_config(&mut self, session_id: &str) -> Result<bool> {
        if let Some(config) = self.storage.get_config(session_id).await? {
            self.current_config = Some(config);
            self.session_id = Some(session_id.to_string());
            tracing::info!("Loaded configuration for session: {}", session_id);
            Ok(true)
        } else {
            tracing::warn!("No configuration found for session: {}", session_id);
            Ok(false)
        }
    }

    /// Get current configuration
    pub fn get_config(&self) -> Option<&AgentConfig> {
        self.current_config.as_ref()
    }

    /// Get current session ID
    pub fn get_session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Check if configuration is loaded
    pub fn is_loaded(&self) -> bool {
        self.current_config.is_some()
    }

    /// Get the next service in the pipeline for the current service
    pub fn get_next_service(&self, current_service: &str) -> Option<(&str, &str)> {
        self.current_config
            .as_ref()
            .and_then(|config| config.get_next_service(current_service))
            .map(|step| (step.to.as_str(), step.stream.as_str()))
    }

    /// Get the previous service in the pipeline for the current service
    pub fn get_previous_service(&self, current_service: &str) -> Option<(&str, &str)> {
        self.current_config
            .as_ref()
            .and_then(|config| config.get_previous_service(current_service))
            .map(|step| (step.from.as_str(), step.stream.as_str()))
    }

    /// Get connector configuration for a service
    pub fn get_connector_config(
        &self,
        service_name: &str,
    ) -> Option<&crate::config::ConnectorConfig> {
        self.current_config
            .as_ref()
            .and_then(|config| config.get_connector_config(service_name))
    }

    /// Get system prompt
    pub fn get_system_prompt(&self) -> Option<&str> {
        self.current_config
            .as_ref()
            .map(|config| config.system_prompt.as_str())
    }

    /// Clear current configuration
    pub fn clear_config(&mut self) {
        self.current_config = None;
        self.session_id = None;
        tracing::info!("Cleared current configuration");
    }

    /// Extend TTL for current session
    pub async fn extend_ttl(&self, ttl_seconds: u64) -> Result<()> {
        if let Some(session_id) = &self.session_id {
            self.storage
                .extend_config_ttl(session_id, ttl_seconds)
                .await?;
        }
        Ok(())
    }

    /// Get TTL for current session
    pub async fn get_ttl(&self) -> Result<Option<i64>> {
        if let Some(session_id) = &self.session_id {
            self.storage.get_config_ttl(session_id).await
        } else {
            Ok(None)
        }
    }
}
