use limitr::{ServerConfig, start_server};
use limitr::config::loader::load_config;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing/logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "limitr=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Rate Limiter Service Starting...");

    // Load and validate configuration
    tracing::info!("Loading configuration...");
    let app_config = load_config().await?;
    tracing::info!("Configuration loaded successfully!");

    // Build configuration cache
    let _config_cache = limitr::config::loader::build_config_cache(&app_config);
    tracing::info!("Configuration cache built with {} domains", 
        app_config.rate_limits.domains.len());

    // Load server configuration from environment
    let server_config = ServerConfig::from_env();
    
    tracing::info!("Server will listen on: {}", server_config.addr());

    // Start the gRPC server
    start_server(server_config).await?;

    Ok(())
}