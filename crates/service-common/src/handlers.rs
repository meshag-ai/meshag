//! HTTP handlers for Kubernetes and infrastructure endpoints

use crate::types::{HealthCheck, HealthResponse, ReadinessResponse};
use axum::{extract::State, http::StatusCode, Json};
use meshag_shared::EventQueue;
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use std::time::SystemTime;

/// Trait for service-specific state
#[async_trait::async_trait]
pub trait ServiceState: Send + Sync {
    fn service_name(&self) -> String;
    async fn is_ready(&self) -> Vec<HealthCheck>;
    async fn get_metrics(&self) -> Vec<String>;
    fn event_queue(&self) -> &EventQueue;
}

static START_TIME: once_cell::sync::Lazy<SystemTime> = once_cell::sync::Lazy::new(SystemTime::now);

/// Generic health check endpoint
pub async fn health_check<S: ServiceState>(
    State(state): State<Arc<S>>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let uptime = START_TIME
        .elapsed()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs();

    let response = HealthResponse {
        status: "healthy".to_string(),
        service: state.service_name(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    };

    Ok(Json(response))
}

/// Generic readiness check endpoint
pub async fn readiness_check<S: ServiceState>(
    State(state): State<Arc<S>>,
) -> Result<Json<ReadinessResponse>, StatusCode> {
    let nats_status = match state.event_queue().health_check().await {
        Ok(true) => HealthCheck {
            name: "nats_connection".to_string(),
            status: "healthy".to_string(),
            message: None,
        },
        _ => HealthCheck {
            name: "nats_connection".to_string(),
            status: "unhealthy".to_string(),
            message: Some("Cannot connect to NATS".to_string()),
        },
    };

    let mut checks = state.is_ready().await;
    checks.push(nats_status);

    let is_ready = checks.iter().all(|c| c.status == "healthy");

    let response = ReadinessResponse {
        status: if is_ready { "ready" } else { "not_ready" }.to_string(),
        service: state.service_name(),
        checks,
    };

    Ok(Json(response))
}

/// Generic metrics endpoint for Prometheus scraping
pub async fn metrics<S: ServiceState>(State(state): State<Arc<S>>) -> Result<String, StatusCode> {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    let service_metrics = state.get_metrics().await;
    let base_metrics = get_base_metrics(state.service_name());
    let all_metrics = [base_metrics, service_metrics].concat();

    let gathered_metrics = prometheus::gather();
    encoder
        .encode(&gathered_metrics, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let custom_metrics = all_metrics.join("\n");
    buffer.extend_from_slice(custom_metrics.as_bytes());
    buffer.push(b'\n');

    Ok(String::from_utf8(buffer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
}

fn get_base_metrics(service_name: String) -> Vec<String> {
    let uptime = START_TIME.elapsed().unwrap().as_secs();
    vec![
        format!(
            "# HELP {}_uptime_seconds Service uptime in seconds",
            service_name
        ),
        format!("# TYPE {}_uptime_seconds counter", service_name),
        format!("{}_uptime_seconds {}", service_name, uptime),
    ]
}
