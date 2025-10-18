use crate::config::RedisConfig;
use crate::errors::{RateLimitError, Result};
use deadpool_redis::{Config as DeadpoolRedisConfig, Pool, Runtime};
use deadpool::managed::PoolConfig as DeadpoolPoolConfig;
use redis::RedisError;
use tracing::{info, debug};

/// Create a Redis connection pool from configuration
pub async fn create_redis_pool(config: &RedisConfig) -> Result<Pool> {
    info!("Creating Redis connection pool...");

    // Build deadpool-redis config from URL
    let mut cfg = DeadpoolRedisConfig::from_url(config.url.clone());

    // Set pool sizing
    cfg.pool = Some(DeadpoolPoolConfig::new(config.max_connections));

    // Create the pool
    let pool = cfg
        .create_pool(Some(Runtime::Tokio1))
        .map_err(|e| RateLimitError::RedisConnectionError(
            RedisError::from((redis::ErrorKind::IoError, "Pool creation failed", e.to_string()))
        ))?;

    info!(
        "Redis connection pool created (max_connections: {})",
        config.max_connections
    );

    // Test connection
    debug!("Testing Redis connection...");
    let mut conn = pool.get().await
        .map_err(|e| RateLimitError::RedisConnectionError(
            RedisError::from((redis::ErrorKind::IoError, "Failed to get connection", e.to_string()))
        ))?;

    // Correct usage: return type first. Let connection type be inferred.
    let _pong: String = redis::cmd("PING")
        .query_async(&mut *conn)
        .await
        .map_err(|e| RateLimitError::RedisConnectionError(e))?;

    info!("Redis connection test successful");

    Ok(pool)
}

/// Get pool statistics
pub fn get_pool_stats(pool: &Pool) -> PoolStats {
    let status = pool.status();
    PoolStats {
        size: status.size,
        available: status.available,
        max_size: status.max_size,
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub size: usize,
    pub available: usize,
    pub max_size: usize,
}
