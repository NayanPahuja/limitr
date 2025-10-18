use limitr::{ServerConfig};
use limitr::config::loader::{load_config, build_config_cache};
use limitr::config::watcher::{watch_config_file};
use limitr::redis::pool::create_redis_pool;
use limitr::redis::client::RedisClientImpl;
use limitr::limiter::leaky_bucket::LeakyBucketLimiter;
use std::sync::Arc;
use std::path::PathBuf; // NEW: Import PathBuf
use arc_swap::ArcSwap; // NEW: Import ArcSwap
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
    let initial_cache = build_config_cache(&app_config);
    // CHANGED: Wrap the cache in ArcSwap to allow for hot-reloading
    let config_cache = Arc::new(ArcSwap::new(Arc::new(initial_cache)));
    tracing::info!("Configuration cache built with {} domains",
        app_config.rate_limits.domains.len());

    // NEW: Spawn the configuration watcher as a background task
    let config_path_str = std::env::var("RATE_LIMIT_CONFIG")
        .unwrap_or_else(|_| "config/rate_limits.json".to_string());
    let watcher_cache_clone = Arc::clone(&config_cache);

    tokio::spawn(async move {
        tracing::info!("Starting configuration watcher for '{}'", &config_path_str);
        let path = PathBuf::from(config_path_str);
        if let Err(e) = watch_config_file(path, watcher_cache_clone).await {
            tracing::error!("Configuration watcher failed and has terminated: {}", e);
        }
    });
    // END NEW

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
    // CHANGED: Pass the new Arc<ArcSwap<ConfigCache>> type
    limitr::server::start_server(server_config, limiter, config_cache).await?;

    Ok(())
}