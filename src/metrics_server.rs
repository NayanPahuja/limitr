use crate::errors::Result as errResult;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, error};

/// Start HTTP server for Prometheus metrics endpoint
pub async fn start_metrics_server(port: u16) -> errResult<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await
        .map_err(|e| crate::errors::RateLimitError::InternalError(
            format!("Failed to bind metrics server: {}", e)
        ))?;
    
    info!("Metrics server listening on http://0.0.0.0:{}/metrics", port);
    
    loop {
        match listener.accept().await {
            Ok((mut socket, _)) => {
                tokio::spawn(async move {
                    let mut buffer = [0; 1024];
                    
                    match socket.read(&mut buffer).await {
                        Ok(_) => {
                            // Parse request to check if it's GET /metrics
                            let request = String::from_utf8_lossy(&buffer);
                            
                            if request.contains("GET /metrics") || request.contains("GET / ") {
                                // Gather metrics
                                match gather_metrics_safe() {
                                    Ok(metrics) => {
                                        let response = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                                            metrics.len(),
                                            metrics
                                        );
                                        let _ = socket.write_all(response.as_bytes()).await;
                                    }
                                    Err(e) => {
                                        error!("Failed to gather metrics: {}", e);
                                        let response = "HTTP/1.1 500 Internal Server Error\r\n\r\n";
                                        let _ = socket.write_all(response.as_bytes()).await;
                                    }
                                }
                            } else {
                                let response = "HTTP/1.1 404 Not Found\r\n\r\nTry GET /metrics";
                                let _ = socket.write_all(response.as_bytes()).await;
                            }
                        }
                        Err(e) => {
                            error!("Failed to read from socket: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

/// Gather metrics with Send-safe error type
fn gather_metrics_safe() -> Result<String, String> {
    use prometheus::{Encoder, TextEncoder};
    
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    
    encoder.encode(&metric_families, &mut buffer)
        .map_err(|e| format!("Failed to encode metrics: {}", e))?;
    
    String::from_utf8(buffer)
        .map_err(|e| format!("Failed to convert metrics to UTF-8: {}", e))
}