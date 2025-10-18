pub mod client;
pub mod pool;
pub mod script;

use crate::errors::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]

pub struct ScriptResponse {
    /// Whether request was allowed (1) or denied (0)
    pub allowed: bool,
    
    /// Remaining capacity or deny_count (depending on allowed)
    pub capacity_or_deny: f64,
    
    /// Which rate policy was most restrictive (0-indexed)
    pub limiting_rate_index: i32,
}

// Redis Client Trait async
#[async_trait]
pub trait RedisClient: Send + Sync {
    /// Execute the leaky bucket Lua script
    async fn execute_rate_limit_script(
        &self,
        key: &str,
        cost: i32,
        policies: &[(f64, i64)], // (flow_rate, burst_capacity) pairs
    ) -> Result<ScriptResponse>;
    
    /// Get bucket state from Redis (MessagePack encoded)
    async fn get_bucket_state(&self, key: &str) -> Result<Option<Vec<u8>>>;
    
    /// Check if Redis is healthy
    async fn health_check(&self) -> Result<()>;
}