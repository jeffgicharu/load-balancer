//! Prometheus metrics HTTP server.
//!
//! Serves metrics on a configurable HTTP endpoint.

use crate::metrics::MetricsCollector;
use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prometheus_client::encoding::text::encode;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Prometheus metrics HTTP server.
pub struct MetricsServer {
    /// Address to bind.
    address: SocketAddr,
    /// Path for metrics endpoint.
    path: String,
    /// Metrics collector.
    collector: MetricsCollector,
}

impl MetricsServer {
    /// Create a new metrics server.
    pub fn new(address: SocketAddr, path: String, collector: MetricsCollector) -> Self {
        Self {
            address,
            path,
            collector,
        }
    }

    /// Run the metrics server.
    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) {
        let listener = match TcpListener::bind(self.address).await {
            Ok(l) => l,
            Err(e) => {
                error!(error = %e, address = %self.address, "failed to bind metrics server");
                return;
            }
        };

        info!(address = %self.address, path = %self.path, "metrics server started");

        let collector = Arc::new(self.collector);
        let path = Arc::new(self.path);

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let collector = Arc::clone(&collector);
                            let path = Arc::clone(&path);

                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                let service = service_fn(move |req| {
                                    let collector = Arc::clone(&collector);
                                    let path = Arc::clone(&path);
                                    async move {
                                        handle_request(req, &collector, &path).await
                                    }
                                });

                                if let Err(e) = http1::Builder::new()
                                    .serve_connection(io, service)
                                    .await
                                {
                                    debug!(error = %e, "metrics connection error");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "failed to accept metrics connection");
                        }
                    }
                }

                _ = shutdown.recv() => {
                    info!("metrics server shutting down");
                    break;
                }
            }
        }
    }
}

/// Handle an incoming metrics request.
async fn handle_request(
    req: Request<hyper::body::Incoming>,
    collector: &MetricsCollector,
    metrics_path: &str,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path();
    let method = req.method();

    debug!(path = %path, method = %method, "metrics request");

    // Only handle GET requests to the metrics path
    if method != Method::GET {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Full::new(Bytes::from("Method not allowed\n")))
            .unwrap());
    }

    if path == metrics_path {
        // Encode metrics in Prometheus text format
        let mut buffer = String::new();
        if let Err(e) = encode(&mut buffer, collector.registry()) {
            error!(error = %e, "failed to encode metrics");
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from("Failed to encode metrics\n")))
                .unwrap());
        }

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
            .body(Full::new(Bytes::from(buffer)))
            .unwrap())
    } else if path == "/health" || path == "/healthz" {
        // Health check endpoint
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from("OK\n")))
            .unwrap())
    } else if path == "/" {
        // Root path - show simple info
        let body = format!(
            "rustlb metrics server\n\nEndpoints:\n  {} - Prometheus metrics\n  /health - Health check\n",
            metrics_path
        );
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain")
            .body(Full::new(Bytes::from(body)))
            .unwrap())
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not found\n")))
            .unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_server_new() {
        let collector = MetricsCollector::new();
        let server = MetricsServer::new(
            "127.0.0.1:9090".parse().unwrap(),
            "/metrics".to_string(),
            collector,
        );
        assert_eq!(server.address, "127.0.0.1:9090".parse().unwrap());
        assert_eq!(server.path, "/metrics");
    }

    #[test]
    fn test_metrics_encoding() {
        let collector = MetricsCollector::new();

        // Record some metrics
        collector.record_request("web", "api", "GET", 200, std::time::Duration::from_millis(10));
        collector.connection_opened("web", "api");

        // Encode metrics
        let mut buffer = String::new();
        prometheus_client::encoding::text::encode(&mut buffer, collector.registry()).unwrap();

        // Verify output contains expected metrics
        assert!(buffer.contains("rustlb_requests"));
        assert!(buffer.contains("rustlb_active_connections"));
    }
}
