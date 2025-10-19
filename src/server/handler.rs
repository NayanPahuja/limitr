use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::config::ConfigCache;
use crate::generated;
use crate::limiter::RateLimiter;

use generated::{
    BucketLevel as ProtoBucketLevel, CheckRequest, CheckResponse, ConfigRequest, ConfigResponse,
    DomainConfig as ProtoDomainConfig, HealthCheckRequest, HealthCheckResponse,
    RatePolicy as ProtoRatePolicy, StatusRequest, StatusResponse,
    health_check_response::ServingStatus, rate_limiter_service_server::RateLimiterService,
};

pub struct RateLimiterServiceImpl<L: RateLimiter> {
    limiter: Arc<L>,
    config_cache: Arc<ArcSwap<ConfigCache>>,
}

impl<L: RateLimiter> RateLimiterServiceImpl<L> {
    pub fn new(limiter: Arc<L>, config_cache: Arc<ArcSwap<ConfigCache>>) -> Self {
        Self {
            limiter,
            config_cache,
        }
    }
}

#[tonic::async_trait]
impl<L: RateLimiter + 'static> RateLimiterService for RateLimiterServiceImpl<L> {
    async fn consume_and_check_limit(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<CheckResponse>, Status> {
        let req_start = Instant::now();
        let req = request.into_inner();

        // Extract parameters with defaults
        let domain = req.domain.as_deref().unwrap_or("default");
        let limit_key = &req.limit_key;
        let cost = req.cost.unwrap_or(1);

        info!(
            domain = %domain,
            limit_key = %limit_key,
            cost = %cost,
            "Received ConsumeAndCheckLimit request"
        );

        // Extract prefix from limit_key (format: "prefix:actual_key")
        // If no colon, use "default" as prefix
        let (prefix, actual_key) = if let Some(colon_pos) = limit_key.find(':') {
            (&limit_key[..colon_pos], &limit_key[colon_pos + 1..])
        } else {
            ("default", limit_key.as_str())
        };

        // Get rate policies for this domain and prefix
        let policies = self.config_cache.load().get_policies(domain, prefix);

        debug!(
            "Using {} policies for domain '{}' prefix '{}'",
            policies.len(),
            domain,
            prefix
        );

        // Check rate limit
        let decision = self
            .limiter
            .check_limit(domain, prefix, actual_key, &policies, cost)
            .await?;

        let duration = req_start.elapsed().as_secs_f64();
        crate::metrics::record_request(domain, prefix, decision.allowed, duration);
        crate::metrics::record_tokens_consumed(domain, prefix, cost);
        crate::metrics::record_remaining_capacity(domain, prefix, decision.remaining_capacity);
        crate::metrics::record_limiting_index(domain, prefix, decision.limiting_rate_index);

        if !decision.allowed {
            crate::metrics::record_denied(domain, prefix, decision.deny_count);
        }

        let response = CheckResponse {
            allowed: decision.allowed,
            remaining_capacity: decision.remaining_capacity,
            limiting_rate_index: decision.limiting_rate_index,
            deny_count: decision.deny_count,
        };

        debug!(
            allowed = %response.allowed,
            remaining = %response.remaining_capacity,
            "Rate limit decision made"
        );

        Ok(Response::new(response))
    }

    async fn get_current_config(
        &self,
        request: Request<ConfigRequest>,
    ) -> Result<Response<ConfigResponse>, Status> {
        let _req = request.into_inner();

        info!("Received GetCurrentConfig request");

        let config = self.config_cache.load().get_full_config();

        // Convert internal config to proto format
        let proto_configs: Vec<ProtoDomainConfig> = config
            .domains
            .iter()
            .map(|dc| ProtoDomainConfig {
                domain: dc.domain.clone(),
                prefix_key: dc.prefix.clone(),
                policies: dc
                    .policies
                    .iter()
                    .map(|p| ProtoRatePolicy {
                        flow_rate_per_second: p.flow_rate_per_second,
                        burst_capacity: p.burst_capacity,
                        name: p.name.clone(),
                    })
                    .collect(),
            })
            .collect();

        let response = ConfigResponse {
            configs: proto_configs,
        };

        Ok(Response::new(response))
    }

    async fn get_bucket_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = request.into_inner();

        let domain = req.domain.as_deref().unwrap_or("default");
        let limit_key = &req.limit_key;

        info!(
            domain = %domain,
            limit_key = %limit_key,
            "Received GetBucketStatus request"
        );

        // Extract prefix and actual key
        let (prefix, actual_key) = if let Some(colon_pos) = limit_key.find(':') {
            (&limit_key[..colon_pos], &limit_key[colon_pos + 1..])
        } else {
            ("default", limit_key.as_str())
        };

        // Get bucket status
        let status = self
            .limiter
            .get_bucket_status(domain, prefix, actual_key)
            .await?;

        // Convert to proto format
        let proto_levels: Vec<ProtoBucketLevel> = status
            .levels
            .iter()
            .map(|level| ProtoBucketLevel {
                current_level: level.current_level,
                flow_rate: level.flow_rate,
                burst_capacity: level.burst_capacity,
                remaining_capacity: level.remaining_capacity,
            })
            .collect();

        let response = StatusResponse {
            levels: proto_levels,
            last_update_timestamp: status.last_update_timestamp,
            deny_count: status.deny_count,
        };

        Ok(Response::new(response))
    }

    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let _req = request.into_inner();

        debug!("Received HealthCheck request");

        // Simple health check - just check if server is up
        let response = HealthCheckResponse {
            status: ServingStatus::Serving as i32,
            message: "Server is healthy".to_string(),
        };

        Ok(Response::new(response))
    }
}
