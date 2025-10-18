use limitr::{ServerConfig};
use limitr::config::loader::{load_config, build_config_cache};
use limitr::redis::pool::create_redis_pool;
use limitr::redis::client::RedisClientImpl;
use limitr::limiter::leaky_bucket::LeakyBucketLimiter;
use std::sync::Arc;
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
    let config_cache = build_config_cache(&app_config);
    let config_cache = Arc::new(config_cache);
    tracing::info!("Configuration cache built with {} domains", 
        app_config.rate_limits.domains.len());

    // Create Redis connection pool
    tracing::info!("Initializing Redis connection pool...");
    let redis_pool = create_redis_pool(&app_config.redis).await?;
    tracing::info!("Redis connection pool initialized");

    // Create Redis client
    tracing::info!("Loading Redis Lua script...");
    let redis_client = RedisClientImpl::new(redis_pool).await?;
    let redis_client = Arc::new(redis_client);
    tracing::info!("Redis client ready");

    // Create rate limiter
    let limiter = LeakyBucketLimiter::new(redis_client);
    let limiter = Arc::new(limiter);
    tracing::info!("Rate limiter initialized");

    // Load server configuration from environment
    let server_config = ServerConfig::from_env();
    tracing::info!("Server will listen on: {}", server_config.addr());

    // Start the gRPC server
    limitr::server::start_server(server_config, limiter, config_cache).await?;

    Ok(())
}