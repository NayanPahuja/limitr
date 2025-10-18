use crate::errors::{RateLimitError, Result};
use crate::redis::{RedisClient as RedisClientTrait, ScriptResponse};
use crate::redis::script::{load_script, get_script};
use async_trait::async_trait;
use deadpool_redis::Pool;
use redis::AsyncCommands;
use std::sync::Arc;
use tracing::{debug, error};

/// Redis client implementation
pub struct RedisClientImpl {
    pool: Arc<Pool>,
}

impl RedisClientImpl {
    /// Create a new Redis client
    pub async fn new(pool: Pool) -> Result<Self> {
        let pool = Arc::new(pool);
        
        // Load script and get SHA
        let mut conn = pool.get().await
            .map_err(|e| RateLimitError::InternalError(
                format!("Failed to get connection for script loading: {}", e)
            ))?;
        let _sha = load_script(&mut *conn).await?;
        
        Ok(Self {
            pool,
        })
        
    }
}

#[async_trait]
impl RedisClientTrait for RedisClientImpl {
    async fn execute_rate_limit_script(
        &self,
        key: &str,
        cost: i32,
        policies: &[(f64, i64)],
    ) -> Result<ScriptResponse> {
        // Get connection from pool
        let mut conn = self.pool.get().await
            .map_err(|e| {
                error!("Failed to get Redis connection: {}", e);
                RateLimitError::RedisConnectionError(
                    redis::RedisError::from((redis::ErrorKind::IoError, "Pool exhausted", e.to_string()))
                )
            })?;
        
        // Build arguments: [cost, flow1, burst1, flow2, burst2, ...]
        let mut args: Vec<String> = vec![cost.to_string()];
        for (flow_rate, burst_capacity) in policies {
            args.push(flow_rate.to_string());
            args.push(burst_capacity.to_string());
        }
        
        debug!(
            "Executing rate limit script: key={}, cost={}, policies={}",
            key, cost, policies.len()
        );
        
        // Execute script using EVALSHA
        let script = get_script();
        let result: Vec<redis::Value> = script
            .key(key)
            .arg(&args)
            .invoke_async(&mut *conn)
            .await
            .map_err(|e| {
                error!("Script execution failed: {}", e);
                RateLimitError::ScriptExecutionError(format!("Script execution failed: {}", e))
            })?;
        
        // Parse response: [allowed, capacity_or_deny, limiting_index]
        if result.len() != 3 {
            return Err(RateLimitError::ScriptExecutionError(
                format!("Invalid script response length: {}", result.len())
            ));
        }
        
        let allowed = match &result[0] {
            redis::Value::Int(v) => *v == 1,
            _ => return Err(RateLimitError::ScriptExecutionError(
                "Invalid allowed value type".to_string()
            )),
        };
        
        let capacity_or_deny = match &result[1] {
            redis::Value::BulkString(bytes) => {
                let s = std::str::from_utf8(bytes)
                    .map_err(|e| RateLimitError::ScriptExecutionError(
                        format!("Invalid UTF-8 in capacity: {}", e)
                    ))?;
                s.parse::<f64>()
                    .map_err(|e| RateLimitError::ScriptExecutionError(
                        format!("Failed to parse capacity: {}", e)
                    ))?
            }
            _ => return Err(RateLimitError::ScriptExecutionError(
                format!("Invalid capacity value type: {:?}", result[1])
            )),
        };
        
        let limiting_rate_index = match &result[2] {
            redis::Value::Int(v) => *v as i32,
            _ => return Err(RateLimitError::ScriptExecutionError(
                "Invalid limiting_rate_index type".to_string()
            )),
        };
        
        debug!(
            "Script result: allowed={}, capacity_or_deny={:.2}, limiting_index={}",
            allowed, capacity_or_deny, limiting_rate_index
        );
        
        Ok(ScriptResponse {
            allowed,
            capacity_or_deny,
            limiting_rate_index,
        })
    }
    
    async fn get_bucket_state(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut conn = self.pool.get().await
            .map_err(|e| RateLimitError::RedisConnectionError(
                redis::RedisError::from((redis::ErrorKind::IoError, "Pool exhausted", e.to_string()))
            ))?;
        
        let data: Option<Vec<u8>> = conn.get(key).await
            .map_err(|e| RateLimitError::RedisCommandError(e.to_string()))?;
        
        Ok(data)
    }
    
    async fn health_check(&self) -> Result<()> {
        let mut conn = self.pool.get().await
            .map_err(|e| RateLimitError::RedisConnectionError(
                redis::RedisError::from((redis::ErrorKind::IoError, "Pool exhausted", e.to_string()))
            ))?;
        
        let response: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .map_err(|e| RateLimitError::RedisConnectionError(e))?;
        
        if response != "PONG" {
            return Err(RateLimitError::InternalError(
                format!("Unexpected PING response: {}", response)
            ));
        }
        
        Ok(())
    }
}