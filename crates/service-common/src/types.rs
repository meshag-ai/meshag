use serde::{Deserialize, Serialize};

/// Health check response for Kubernetes liveness probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub uptime_seconds: u64,
}

/// Readiness check response for Kubernetes readiness probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub service: String,
    pub checks: Vec<HealthCheck>,
}

/// Individual health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}
