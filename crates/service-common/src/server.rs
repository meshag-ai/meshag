use crate::handlers::{health_check, metrics, readiness_check, ServiceState};
use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn run<S: ServiceState + 'static>(state: Arc<S>, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health_check::<S>))
        .route("/ready", get(readiness_check::<S>))
        .route("/metrics", get(metrics::<S>))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{}", port);
    info!("{} service starting on {}", S::service_name(&state), addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
