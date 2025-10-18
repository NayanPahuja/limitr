pub mod leaky_bucket;

use crate::config::RatePolicy;
use crate::errors::Result;
use async_trait::async_trait;

/// Response from a rate limit check
#[derive(Debug, Clone)]
pub struct LimitDecision {
    /// Whether the request is allowed
    pub allowed: bool,
    
    /// Remaining capacity (can be negative if denied, represents deny_count)
    pub remaining_capacity: f64,
    
    /// Which rate policy was most restrictive (0-indexed)
    pub limiting_rate_index: i32,
    
    /// Total accumulated denied tokens (only when denied)
    pub deny_count: i64,
}

/// Bucket status without consuming tokens
#[derive(Debug, Clone)]
pub struct BucketStatus {
    /// Status of each rate policy bucket
    pub levels: Vec<BucketLevel>,
    
    /// Last update timestamp (Unix timestamp in seconds)
    pub last_update_timestamp: i64,
    
    /// Total accumulated denied tokens
    pub deny_count: i64,
}

/// Status of individual bucket (per policy)
#[derive(Debug, Clone)]
pub struct BucketLevel {
    pub current_level: f64,
    pub flow_rate: f64,
    pub burst_capacity: i64,
    pub remaining_capacity: f64,
}

/// Trait for rate limiting algorithms
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if request is allowed and consume tokens
    async fn check_limit(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
        policies: &[RatePolicy],
        cost: i32,
    ) -> Result<LimitDecision>;
    
    /// Get bucket status without consuming tokens
    async fn get_bucket_status(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
    ) -> Result<BucketStatus>;
}