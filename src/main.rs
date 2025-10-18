use limitr::{ServerConfig, start_server};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing/logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rate_limiter=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Rate Limiter Service Starting...");

    // Load server configuration from environment
    let config = ServerConfig::from_env();
    
    tracing::info!("Server will listen on: {}", config.addr());

    // Start the gRPC server
    start_server(config).await?;

    Ok(())
}