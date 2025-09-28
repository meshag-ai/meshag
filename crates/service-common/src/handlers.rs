//! HTTP handlers for Kubernetes and infrastructure endpoints

use crate::types::HealthResponse;
use axum::{extract::State, http::StatusCode, Json};
use meshag_shared::EventQueue;
use std::sync::Arc;
use std::time::SystemTime;

#[async_trait::async_trait]
pub trait ServiceState: Send + Sync {
    fn service_name(&self) -> String;
    fn event_queue(&self) -> &EventQueue;
}

static START_TIME: once_cell::sync::Lazy<SystemTime> = once_cell::sync::Lazy::new(SystemTime::now);

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
