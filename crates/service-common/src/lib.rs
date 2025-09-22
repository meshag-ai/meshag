pub mod handlers;
pub mod server;
pub mod types;

// Re-export commonly used types and traits
pub use handlers::ServiceState;
pub use types::{HealthCheck, HealthResponse, ReadinessResponse};
