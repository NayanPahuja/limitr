use crate::config::{AppConfig, DomainConfig, RatePolicy, RedisConfig};
use crate::errors::{RateLimitError, Result};
use tracing::{debug, warn};

/// Validate the entire application configuration
pub fn validate_config(config: &AppConfig) -> Result<()> {
    debug!("Validating configuration...");
    
    // Validate Redis configuration
    validate_redis_config(&config.redis)?;
    
    // Validate rate limit configurations
    for domain_config in &config.rate_limits.domains {
        validate_domain_config(domain_config)?;
    }
    
    // Validate default configuration
    validate_domain_config(&config.rate_limits.default)?;
    
    debug!("Configuration validation successful");
    Ok(())
}

/// Validate Redis configuration
fn validate_redis_config(config: &RedisConfig) -> Result<()> {
    // Validate URL format
    if config.url.is_empty() {
        return Err(RateLimitError::ConfigurationError(
            "Redis URL cannot be empty".to_string()
        ));
    }
    
    // Basic URL validation (should start with redis:// or rediss://)
    if !config.url.starts_with("redis://") && !config.url.starts_with("rediss://") {
        return Err(RateLimitError::ConfigurationError(
            format!("Invalid Redis URL format: {}. Must start with redis:// or rediss://", config.url)
        ));
    }
    
    // Validate connection pool size
    if config.max_connections == 0 {
        return Err(RateLimitError::ConfigurationError(
            "max_connections must be greater than 0".to_string()
        ));
    }
    
    if config.max_connections > 1000 {
        warn!(
            "max_connections is very high ({}). This may consume excessive resources.",
            config.max_connections
        );
    }
    
    // Validate timeouts
    if config.connection_timeout_secs == 0 {
        return Err(RateLimitError::ConfigurationError(
            "connection_timeout_secs must be greater than 0".to_string()
        ));
    }
    
    if config.command_timeout_secs == 0 {
        return Err(RateLimitError::ConfigurationError(
            "command_timeout_secs must be greater than 0".to_string()
        ));
    }
    
    debug!("Redis configuration valid");
    Ok(())
}

/// Validate domain configuration
fn validate_domain_config(config: &DomainConfig) -> Result<()> {
    // Validate domain name
    if config.domain.is_empty() {
        return Err(RateLimitError::ConfigurationError(
            "Domain name cannot be empty".to_string()
        ));
    }
    
    // Validate prefix
    if config.prefix.is_empty() {
        return Err(RateLimitError::ConfigurationError(
            format!("Prefix cannot be empty for domain '{}'", config.domain)
        ));
    }
    
    // Must have at least one policy
    if config.policies.is_empty() {
        return Err(RateLimitError::ConfigurationError(
            format!("Domain '{}' must have at least one rate policy", config.domain)
        ));
    }
    
    // Validate each policy
    for (idx, policy) in config.policies.iter().enumerate() {
        validate_rate_policy(policy, &config.domain, idx)?;
    }
    
    debug!("Domain configuration valid for '{}'", config.domain);
    Ok(())
}

/// Validate individual rate policy
fn validate_rate_policy(policy: &RatePolicy, domain: &str, index: usize) -> Result<()> {
    // Validate name
    if policy.name.is_empty() {
        return Err(RateLimitError::InvalidRate(
            format!("Policy name cannot be empty for domain '{}' policy {}", domain, index)
        ));
    }
    
    // Validate flow rate
    if policy.flow_rate_per_second <= 0.0 {
        return Err(RateLimitError::InvalidRate(
            format!(
                "flow_rate_per_second must be positive for domain '{}' policy '{}' (got {})",
                domain, policy.name, policy.flow_rate_per_second
            )
        ));
    }
    
    if policy.flow_rate_per_second > 1_000_000.0 {
        warn!(
            "Very high flow_rate_per_second ({}) for domain '{}' policy '{}'",
            policy.flow_rate_per_second, domain, policy.name
        );
    }
    
    // Validate burst capacity
    if policy.burst_capacity <= 0 {
        return Err(RateLimitError::InvalidRate(
            format!(
                "burst_capacity must be positive for domain '{}' policy '{}' (got {})",
                domain, policy.name, policy.burst_capacity
            )
        ));
    }
    
    if policy.burst_capacity > 1_000_000_000 {
        warn!(
            "Very high burst_capacity ({}) for domain '{}' policy '{}'",
            policy.burst_capacity, domain, policy.name
        );
    }
    
    // Check if burst capacity is reasonable relative to flow rate
    let seconds_to_fill = policy.burst_capacity as f64 / policy.flow_rate_per_second;
    if seconds_to_fill < 1.0 {
        warn!(
            "Burst capacity for domain '{}' policy '{}' is very small relative to flow rate (fills in {:.2}s)",
            domain, policy.name, seconds_to_fill
        );
    }
    
    if seconds_to_fill > 86400.0 {  // More than 24 hours
        warn!(
            "Burst capacity for domain '{}' policy '{}' is very large relative to flow rate (takes {:.2} hours to fill)",
            domain, policy.name, seconds_to_fill / 3600.0
        );
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_config() {
        let config = AppConfig {
            redis: RedisConfig::default(),
            rate_limits: crate::config::RateLimitConfig::default(),
        };
        
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_invalid_redis_url() {
        let mut config = AppConfig {
            redis: RedisConfig::default(),
            rate_limits: crate::config::RateLimitConfig::default(),
        };
        
        config.redis.url = "invalid_url".to_string();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_zero_flow_rate() {
        let policy = RatePolicy {
            name: "test".to_string(),
            flow_rate_per_second: 0.0,
            burst_capacity: 100,
        };
        
        assert!(validate_rate_policy(&policy, "test_domain", 0).is_err());
    }

    #[test]
    fn test_validate_negative_burst() {
        let policy = RatePolicy {
            name: "test".to_string(),
            flow_rate_per_second: 10.0,
            burst_capacity: -100,
        };
        
        assert!(validate_rate_policy(&policy, "test_domain", 0).is_err());
    }
}