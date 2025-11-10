pub mod handlers;
pub mod state;
pub mod websocket;

use anyhow::Result;
use axum::{extract::ws::WebSocketUpgrade, routing::get, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::info;

use state::GatewayState;

pub async fn run_gateway_service() -> Result<()> {
    let state = GatewayState::new().await?;
    info!("API Gateway initialized successfully");

    let app = create_app_router(state);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting API Gateway on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_app_router(state: Arc<GatewayState>) -> Router {
    Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(handlers::health_check))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| websocket::handle_websocket(socket, state))
}
