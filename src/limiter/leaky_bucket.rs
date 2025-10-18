use crate::config::RatePolicy;
use crate::errors::{RateLimitError, Result};
use crate::limiter::{RateLimiter, LimitDecision, BucketStatus, BucketLevel};
use crate::redis::RedisClient;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, warn, error};

/// Leaky bucket rate limiter implementation
pub struct LeakyBucketLimiter<R: RedisClient> {
    redis_client: Arc<R>,
}

impl<R: RedisClient> LeakyBucketLimiter<R> {
    /// Create a new leaky bucket limiter
    pub fn new(redis_client: Arc<R>) -> Self {
        Self { redis_client }
    }
    
    /// Construct Redis key for bucket
    fn construct_bucket_key(&self, domain: &str, prefix: &str, key: &str) -> String {
        format!("bucket:{}:{}:{}", domain, prefix, key)
    }
}

#[async_trait]
impl<R: RedisClient + 'static> RateLimiter for LeakyBucketLimiter<R> {
    async fn check_limit(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
        policies: &[RatePolicy],
        cost: i32,
    ) -> Result<LimitDecision> {
        let bucket_key = self.construct_bucket_key(domain, prefix, key);
        
        debug!(
            "Checking rate limit: domain={}, prefix={}, key={}, policies={}, cost={}",
            domain, prefix, key, policies.len(), cost
        );
        
        // Convert policies to (flow_rate, burst_capacity) pairs
        let policy_pairs: Vec<(f64, i64)> = policies
            .iter()
            .map(|p| (p.flow_rate_per_second, p.burst_capacity))
            .collect();
        
        // Execute Redis script
        match self.redis_client
            .execute_rate_limit_script(&bucket_key, cost, &policy_pairs)
            .await
        {
            Ok(response) => {
                let decision = if response.allowed {
                    LimitDecision {
                        allowed: true,
                        remaining_capacity: response.capacity_or_deny,
                        limiting_rate_index: response.limiting_rate_index,
                        deny_count: 0,
                    }
                } else {
                    LimitDecision {
                        allowed: false,
                        remaining_capacity: -response.capacity_or_deny,
                        limiting_rate_index: response.limiting_rate_index,
                        deny_count: response.capacity_or_deny as i64,
                    }
                };
                
                debug!(
                    "Rate limit decision: allowed={}, remaining={:.2}, limiting_policy={}",
                    decision.allowed,
                    decision.remaining_capacity,
                    decision.limiting_rate_index
                );
                
                Ok(decision)
            }
            Err(e) => {
                error!("Redis error during rate limit check: {}", e);
                
                // Fail-open strategy: allow request when Redis is unavailable
                warn!(
                    "Failing open due to Redis error for key '{}': {}",
                    bucket_key, e
                );
                
                Ok(LimitDecision {
                    allowed: true,
                    remaining_capacity: f64::MAX,
                    limiting_rate_index: 0,
                    deny_count: 0,
                })
            }
        }
    }
    
    async fn get_bucket_status(
        &self,
        domain: &str,
        prefix: &str,
        key: &str,
    ) -> Result<BucketStatus> {
        let bucket_key = self.construct_bucket_key(domain, prefix, key);
        
        debug!("Getting bucket status for: {}", bucket_key);
        
        // Get raw bucket state from Redis
        let state_data = self.redis_client
            .get_bucket_state(&bucket_key)
            .await?;
        
        match state_data {
            Some(data) => {
                // Decode MessagePack data
                match Self::decode_bucket_state(&data) {
                    Ok((last_time, deny_count, token_levels)) => {
                        // Create bucket levels (we don't have policy info here, so provide minimal data)
                        let levels = token_levels
                            .iter()
                            .map(|&level| BucketLevel {
                                current_level: level,
                                flow_rate: 0.0, // Not stored in state
                                burst_capacity: 0, // Not stored in state
                                remaining_capacity: 0.0, // Cannot calculate without policy info
                            })
                            .collect();
                        
                        Ok(BucketStatus {
                            levels,
                            last_update_timestamp: last_time as i64,
                            deny_count,
                        })
                    }
                    Err(e) => {
                        warn!("Failed to decode bucket state: {}", e);
                        // Return empty status
                        Ok(BucketStatus {
                            levels: vec![],
                            last_update_timestamp: 0,
                            deny_count: 0,
                        })
                    }
                }
            }
            None => {
                // Bucket doesn't exist yet
                debug!("Bucket not found: {}", bucket_key);
                Ok(BucketStatus {
                    levels: vec![],
                    last_update_timestamp: 0,
                    deny_count: 0,
                })
            }
        }
    }
}

impl<R: RedisClient> LeakyBucketLimiter<R> {
    /// Decode MessagePack bucket state
    fn decode_bucket_state(data: &[u8]) -> Result<(f64, i64, Vec<f64>)> {
        // Decode MessagePack: [last_time, deny_count, [levels...]]
        let decoded: (f64, i64, Vec<f64>) = rmp_serde::from_slice(data)
            .map_err(|e| RateLimitError::SerializationError(
                format!("Failed to decode bucket state: {}", e)
            ))?;
        
        Ok(decoded)
    }
}