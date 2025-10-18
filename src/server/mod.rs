pub mod handler;

use arc_swap::ArcSwap;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tracing::info;
use std::sync::Arc;

use crate::errors::Result;
use crate::config::ConfigCache;
use crate::limiter::RateLimiter;

/// gRPC server configuration
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 50051,
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("GRPC_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("GRPC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(50051),
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Start the gRPC server with reflection support
pub async fn start_server<L: RateLimiter + 'static>(
    config: ServerConfig,
    limiter: Arc<L>,
    config_cache: Arc<ArcSwap<ConfigCache>>,
) -> Result<()> {
    let addr = config.addr().parse()
        .map_err(|e| crate::errors::RateLimitError::InternalError(
            format!("Invalid server address: {}", e)
        ))?;

    info!("Starting gRPC server on {}", addr);

    let rate_limiter_service = handler::RateLimiterServiceImpl::new(limiter, config_cache);

    // Load the file descriptor set for reflection
    let file_descriptor_set = include_bytes!("../gen/limitr_descriptor.bin");
    
    // Build the reflection service
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(file_descriptor_set)
        .build_v1()
        .map_err(|e| crate::errors::RateLimitError::InternalError(
            format!("Failed to build reflection service: {}", e)
        ))?;

    info!("gRPC reflection enabled");

    // Start server with both the main service and reflection
    Server::builder()
        .add_service(crate::generated::rate_limiter_service_server::RateLimiterServiceServer::new(rate_limiter_service))
        .add_service(reflection_service)
        .serve(addr)
        .await
        .map_err(|e| crate::errors::RateLimitError::InternalError(
            format!("Server error: {}", e)
        ))?;

    Ok(())
}