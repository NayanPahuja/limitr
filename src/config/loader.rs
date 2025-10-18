use crate::config::{AppConfig, RedisConfig, RateLimitConfig, ConfigCache};
use crate::config::validator::validate_config;
use crate::errors::{RateLimitError, Result};
use std::path::Path;
use tracing::{info, debug};

/// Load rate limit configuration from JSON file
pub async fn load_rate_limit_config_from_file<P: AsRef<Path>>(path: P) -> Result<RateLimitConfig> {
    let path = path.as_ref();
    info!("Loading rate limit configuration from: {}", path.display());

    // Read file contents
    let contents = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| RateLimitError::FileSystemError(e))?;

    // Parse JSON
    let config: RateLimitConfig = serde_json::from_str(&contents)
        .map_err(|e| RateLimitError::JsonError(e))?;

    // Validate the rate-limit config by embedding into a temporary AppConfig
    // so we can reuse the existing validate_config which accepts AppConfig.
    let tmp_app_config = AppConfig {
        redis: RedisConfig::default(),
        rate_limits: config.clone(),
    };
    validate_config(&tmp_app_config)?;

    info!("Rate limit configuration loaded and validated successfully");
    log_rate_limit_config_summary(&config);

    Ok(config)
}

/// Load complete application configuration
/// - Redis config from environment variables
/// - Rate limit config from JSON file
pub async fn load_config() -> Result<AppConfig> {
    info!("Loading application configuration...");

    // Load Redis configuration from environment
    info!("Loading Redis configuration from environment variables...");
    let redis_config = RedisConfig::from_env();
    log_redis_config_summary(&redis_config);

    // Get rate limit config file path from environment
    let config_path = std::env::var("RATE_LIMIT_CONFIG")
        .unwrap_or_else(|_| "config/rate_limits.json".to_string());

    debug!("Rate limit config path: {}", config_path);

    // Load rate limit configuration from file
    let rate_limit_config = load_rate_limit_config_from_file(&config_path).await?;

    // Combine configurations
    let app_config = AppConfig {
        redis: redis_config,
        rate_limits: rate_limit_config,
    };

    // Validate the full configuration
    validate_config(&app_config)?;

    info!("Application configuration loaded and validated successfully");
    log_config_summary(&app_config);

    Ok(app_config)
}

/// Build a ConfigCache from AppConfig
pub fn build_config_cache(config: &AppConfig) -> ConfigCache {
    ConfigCache::new(config.rate_limits.clone())
}

/// Log a summary of the loaded configuration (AppConfig)
fn log_config_summary(config: &AppConfig) {
    info!("=== Configuration Summary ===");

    // Redis config
    let redis_url_safe = mask_password(&config.redis.url);
    info!("Redis URL: {}", redis_url_safe);
    info!("Redis TLS: {}", config.redis.use_tls);
    info!("Redis Max Connections: {}", config.redis.max_connections);
    info!("Redis Connection Timeout: {}s", config.redis.connection_timeout_secs);
    info!("Redis Command Timeout: {}s", config.redis.command_timeout_secs);

    // Rate limit config
    info!("Rate Limit Domains: {}", config.rate_limits.domains.len());

    for domain_config in &config.rate_limits.domains {
        info!(
            "  Domain: {} (prefix: {}, policies: {})",
            domain_config.domain,
            domain_config.prefix,
            domain_config.policies.len()
        );

        for policy in &domain_config.policies {
            info!(
                "    - {}: {:.2} tokens/sec, burst: {}",
                policy.name,
                policy.flow_rate_per_second,
                policy.burst_capacity
            );
        }
    }

    // Default policy
    info!(
        "Default Policy: {} (prefix: {}, policies: {})",
        config.rate_limits.default.domain,
        config.rate_limits.default.prefix,
        config.rate_limits.default.policies.len()
    );

    for policy in &config.rate_limits.default.policies {
        info!(
            "  - {}: {:.2} tokens/sec, burst: {}",
            policy.name,
            policy.flow_rate_per_second,
            policy.burst_capacity
        );
    }

    info!("=============================");
}

/// Log only the RateLimitConfig summary (used by rate-limit-only loader)
fn log_rate_limit_config_summary(config: &RateLimitConfig) {
    debug!("=== RateLimitConfig Summary ===");
    debug!("Rate Limit Domains: {}", config.domains.len());

    for domain_config in &config.domains {
        debug!(
            "  Domain: {} (prefix: {}, policies: {})",
            domain_config.domain,
            domain_config.prefix,
            domain_config.policies.len()
        );

        for policy in &domain_config.policies {
            debug!(
                "    - {}: {:.2} tokens/sec, burst: {}",
                policy.name,
                policy.flow_rate_per_second,
                policy.burst_capacity
            );
        }
    }

    debug!(
        "Default Policy: {} (prefix: {}, policies: {})",
        config.default.domain,
        config.default.prefix,
        config.default.policies.len()
    );

    for policy in &config.default.policies {
        debug!(
            "  - {}: {:.2} tokens/sec, burst: {}",
            policy.name,
            policy.flow_rate_per_second,
            policy.burst_capacity
        );
    }
    debug!("===============================");
}

/// Log a summary of Redis config only (safe - masks password)
fn log_redis_config_summary(config: &RedisConfig) {
    let redis_url_safe = mask_password(&config.url);
    info!("Redis URL: {}", redis_url_safe);
    info!("Redis TLS: {}", config.use_tls);
    info!("Redis Max Connections: {}", config.max_connections);
    info!("Redis Connection Timeout: {}s", config.connection_timeout_secs);
    info!("Redis Command Timeout: {}s", config.command_timeout_secs);
}

/// Mask password in Redis URL for safe logging
fn mask_password(url: &str) -> String {
    // Try to find the userinfo separator '@'
    if let Some(at_pos) = url.rfind('@') {
        // Look for ':' before the '@' that separates username and password or the leading ':' for password-only
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            // URL has something like redis://user:password@host:port or redis://:password@host:port
            let mut masked = url.to_string();
            masked.replace_range(colon_pos + 1..at_pos, "***");
            return masked;
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_password() {
        assert_eq!(
            mask_password("redis://:mypassword@localhost:6379"),
            "redis://:***@localhost:6379"
        );

        assert_eq!(
            mask_password("redis://localhost:6379"),
            "redis://localhost:6379"
        );

        assert_eq!(
            mask_password("rediss://user:secret@redis.example.com:6380"),
            "rediss://user:***@redis.example.com:6380"
        );
    }
}
