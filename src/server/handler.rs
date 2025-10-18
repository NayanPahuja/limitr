use tonic::{Request, Response, Status};
use tracing::{info,debug};

use crate::generated;

use generated::{CheckRequest,CheckResponse,ConfigRequest, ConfigResponse, StatusRequest, StatusResponse, HealthCheckRequest, HealthCheckResponse, health_check_response::ServingStatus, rate_limiter_service_server::RateLimiterService};

#[derive(Debug, Default)]
pub struct RateLimiterServiceImpl {}

impl RateLimiterServiceImpl {
    pub fn new() -> Self {
        Self {}
    }
}

#[tonic::async_trait]
impl RateLimiterService for RateLimiterServiceImpl {
    async fn consume_and_check_limit(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<CheckResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            domain = ?req.domain,
            limit_key = %req.limit_key,
            cost = ?req.cost,
            "Received ConsumeAndCheckLimit request"
        );

        // Log the full request body for debugging
        debug!("Request body: {:?}", req);

        // TODO: Implement actual rate limiting logic
        // For now, return a mock response
        let response = CheckResponse {
            allowed: true,
            remaining_capacity: 100.0,
            limiting_rate_index: 0,
            deny_count: 0,
        };

        info!("Responding with: allowed={}", response.allowed);

        Ok(Response::new(response))
    }

    async fn get_current_config(
        &self,
        request: Request<ConfigRequest>,
    ) -> Result<Response<ConfigResponse>, Status> {
        let _req = request.into_inner();
        
        info!("Received GetCurrentConfig request");
        debug!("Request body: {:?}", _req);

        // TODO: Return actual configuration
        let response = ConfigResponse {
            configs: vec![],
        };

        Ok(Response::new(response))
    }

        async fn get_bucket_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            domain = ?req.domain,
            limit_key = %req.limit_key,
            "Received GetBucketStatus request"
        );
        debug!("Request body: {:?}", req);

        // TODO: Return actual bucket status
        let response = StatusResponse {
            levels: vec![],
            last_update_timestamp: 0,
            deny_count: 0,
        };

        Ok(Response::new(response))
    }

       async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let _req = request.into_inner();
        
        debug!("Received HealthCheck request");

        // Simple health check - server is up
        let response = HealthCheckResponse {
            status: ServingStatus::Serving as i32,
            message: "Server is healthy".to_string(),
        };

        Ok(Response::new(response))
    }

}