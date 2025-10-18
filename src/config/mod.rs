pub mod loader;
pub mod validator;
pub mod watcher;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use dashmap::DashMap;

/// Complete application configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Redis configuration (loaded from environment variables only)
    pub redis: RedisConfig,
    
    /// Rate limiting configuration (loaded from file, supports hot reload)
    pub rate_limits: RateLimitConfig,
}

/// Redis connection configuration (loaded from environment variables)
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis cluster URL (e.g., "redis://localhost:6379")
    pub url: String,
    
    /// Use TLS for connection
    pub use_tls: bool,
    
    /// Redis username (optional)
    pub username: Option<String>,
    
    /// Redis password (optional)
    pub password: Option<String>,
    
    /// Maximum number of connections in pool
    pub max_connections: usize,
    
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    
    /// Command timeout in seconds
    pub command_timeout_secs: u64,
}

impl RedisConfig {
    /// Load Redis configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("REDIS_CLUSTER_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            
            use_tls: std::env::var("REDIS_USE_TLS")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(false),
            
            username: std::env::var("REDIS_CLUSTER_USERNAME").ok(),
            
            password: std::env::var("REDIS_CLUSTER_PASSWORD").ok(),
            
            max_connections: std::env::var("REDIS_MAX_CONN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            
            connection_timeout_secs: std::env::var("REDIS_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            
            command_timeout_secs: std::env::var("REDIS_COMMAND_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            use_tls: false,
            username: None,
            password: None,
            max_connections: 50,
            connection_timeout_secs: 5,
            command_timeout_secs: 2,
        }
    }
}

/// Rate limiting configuration (loaded from JSON file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Domain-specific rate limit configurations
    #[serde(default)]
    pub domains: Vec<DomainConfig>,
    
    /// Default rate limit configuration (fallback)
    #[serde(default = "default_rate_policy")]
    pub default: DomainConfig,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            domains: vec![],
            default: default_rate_policy(),
        }
    }
}

/// Configuration for a specific domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainConfig {
    /// Domain name (e.g., "api.example.com" or "default")
    pub domain: String,
    
    /// Prefix for Redis keys (e.g., "user", "admin")
    pub prefix: String,
    
    /// Multiple rate policies for multi-rate limiting
    pub policies: Vec<RatePolicy>,
}

/// Individual rate policy (one bucket in multi-rate system)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatePolicy {
    /// Human-readable name (e.g., "per_second", "per_minute", "hourly")
    pub name: String,
    
    /// Flow rate (tokens leaked per second)
    pub flow_rate_per_second: f64,
    
    /// Maximum bucket capacity (burst size)
    pub burst_capacity: i64,
}

/// Runtime configuration cache with fast concurrent access
pub struct ConfigCache {
    /// Map from (domain, prefix) -> Vec<RatePolicy>
    policies: Arc<DashMap<(String, String), Vec<RatePolicy>>>,
    
    /// Default policy for unknown domains
    default_policy: Arc<Vec<RatePolicy>>,
    
    /// Full configuration (for GetCurrentConfig RPC)
    full_config: Arc<RateLimitConfig>,
}

impl ConfigCache {
    /// Create a new config cache from rate limit configuration
    pub fn new(config: RateLimitConfig) -> Self {
        let policies = DashMap::new();
        
        // Build lookup map
        for domain_config in &config.domains {
            let key = (domain_config.domain.clone(), domain_config.prefix.clone());
            policies.insert(key, domain_config.policies.clone());
        }
        
        Self {
            policies: Arc::new(policies),
            default_policy: Arc::new(config.default.policies.clone()),
            full_config: Arc::new(config),
        }
    }
    
    /// Get rate policies for a specific domain and prefix
    pub fn get_policies(&self, domain: &str, prefix: &str) -> Vec<RatePolicy> {
        let key = (domain.to_string(), prefix.to_string());
        
        self.policies
            .get(&key)
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| self.default_policy.as_ref().clone())
    }
    
    /// Get the full configuration (for observability)
    pub fn get_full_config(&self) -> Arc<RateLimitConfig> {
        Arc::clone(&self.full_config)
    }
    
    /// Get statistics about cached configurations
    pub fn stats(&self) -> ConfigStats {
        ConfigStats {
            domain_count: self.policies.len(),
            default_policy_count: self.default_policy.len(),
        }
    }
}

/// Statistics about configuration cache
#[derive(Debug, Clone)]
pub struct ConfigStats {
    pub domain_count: usize,
    pub default_policy_count: usize,
}

fn default_rate_policy() -> DomainConfig {
    DomainConfig {
        domain: "default".to_string(),
        prefix: "default".to_string(),
        policies: vec![
            RatePolicy {
                name: "per_second".to_string(),
                flow_rate_per_second: 10.0,
                burst_capacity: 100,
            },
            RatePolicy {
                name: "per_minute".to_string(),
                flow_rate_per_second: 100.0 / 60.0,  // ~1.67/sec = 100/min
                burst_capacity: 1000,
            },
        ],
    }
}