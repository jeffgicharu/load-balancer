//! Active health checker.
//!
//! Periodically probes backend servers to verify they are healthy.

use crate::config::{BackendConfig, HealthCheckConfig, HealthCheckType};
use crate::health::HealthState;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::{interval, timeout};
use tracing::{debug, info, warn};

/// Active health checker that probes backend servers.
pub struct HealthChecker {
    /// Health state to update.
    health_state: Arc<HealthState>,
    /// Backend configurations.
    backends: Vec<BackendConfig>,
    /// Default check interval.
    default_interval: Duration,
    /// Default check timeout.
    default_timeout: Duration,
}

impl HealthChecker {
    /// Create a new health checker.
    pub fn new(
        health_state: Arc<HealthState>,
        backends: Vec<BackendConfig>,
        default_interval: Duration,
        default_timeout: Duration,
    ) -> Self {
        Self {
            health_state,
            backends,
            default_interval,
            default_timeout,
        }
    }

    /// Start the health checker background task.
    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) {
        info!("health checker starting");

        // Collect all servers that need checking
        let checks: Vec<(SocketAddr, HealthCheckConfig, Duration)> = self
            .backends
            .iter()
            .filter_map(|backend| {
                backend.health_check.as_ref().map(|check| {
                    backend
                        .servers
                        .iter()
                        .map(|s| {
                            let interval = check.interval.unwrap_or(self.default_interval);
                            (s.address, check.clone(), interval)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .flatten()
            .collect();

        if checks.is_empty() {
            info!("no health checks configured, health checker idle");
            // Just wait for shutdown
            let _ = shutdown.recv().await;
            return;
        }

        // Register all servers
        for (server, _, _) in &checks {
            self.health_state.register_server(*server);
        }

        // Use the smallest interval as the tick rate
        let min_interval = checks
            .iter()
            .map(|(_, _, i)| *i)
            .min()
            .unwrap_or(self.default_interval);

        let mut check_interval = interval(min_interval);
        check_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    // Perform health checks
                    for (server, config, _) in &checks {
                        let server = *server;
                        let config = config.clone();
                        let health_state = Arc::clone(&self.health_state);
                        let check_timeout = config.timeout.unwrap_or(self.default_timeout);

                        // Spawn check in background to not block other checks
                        tokio::spawn(async move {
                            let result = perform_health_check(server, &config, check_timeout).await;
                            match result {
                                Ok(()) => {
                                    debug!(server = %server, "health check passed");
                                    health_state.record_success(server);
                                }
                                Err(e) => {
                                    warn!(server = %server, error = %e, "health check failed");
                                    health_state.record_failure(server);
                                }
                            }
                        });
                    }
                }

                _ = shutdown.recv() => {
                    info!("health checker shutting down");
                    break;
                }
            }
        }
    }
}

/// Perform a single health check on a server.
async fn perform_health_check(
    server: SocketAddr,
    config: &HealthCheckConfig,
    check_timeout: Duration,
) -> Result<(), String> {
    match config.check_type {
        HealthCheckType::Tcp => tcp_health_check(server, check_timeout).await,
        HealthCheckType::Http => {
            let path = config.path.as_deref().unwrap_or("/");
            http_health_check(server, path, config.expected_status, check_timeout).await
        }
    }
}

/// Perform a TCP health check (just connect).
async fn tcp_health_check(server: SocketAddr, check_timeout: Duration) -> Result<(), String> {
    match timeout(check_timeout, TcpStream::connect(server)).await {
        Ok(Ok(_stream)) => Ok(()),
        Ok(Err(e)) => Err(format!("connection failed: {}", e)),
        Err(_) => Err("connection timeout".to_string()),
    }
}

/// Perform an HTTP health check.
async fn http_health_check(
    server: SocketAddr,
    path: &str,
    expected_status: u16,
    check_timeout: Duration,
) -> Result<(), String> {
    // Connect
    let stream = match timeout(check_timeout, TcpStream::connect(server)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("connection failed: {}", e)),
        Err(_) => return Err("connection timeout".to_string()),
    };

    let mut stream = stream;

    // Build simple HTTP request
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, server
    );

    // Send request
    if let Err(e) = stream.write_all(request.as_bytes()).await {
        return Err(format!("write failed: {}", e));
    }

    // Read response (just the status line)
    let mut buf = vec![0u8; 1024];
    let n = match timeout(check_timeout, stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        Ok(Ok(_)) => return Err("empty response".to_string()),
        Ok(Err(e)) => return Err(format!("read failed: {}", e)),
        Err(_) => return Err("read timeout".to_string()),
    };

    // Parse status code from response
    let response = String::from_utf8_lossy(&buf[..n]);
    let status = parse_http_status(&response)?;

    if status == expected_status {
        Ok(())
    } else {
        Err(format!(
            "unexpected status: {} (expected {})",
            status, expected_status
        ))
    }
}

/// Parse HTTP status code from response.
fn parse_http_status(response: &str) -> Result<u16, String> {
    // Format: "HTTP/1.1 200 OK\r\n..."
    let parts: Vec<&str> = response.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("invalid HTTP response".to_string());
    }

    parts[1]
        .parse()
        .map_err(|_| "invalid status code".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_status() {
        assert_eq!(parse_http_status("HTTP/1.1 200 OK\r\n").unwrap(), 200);
        assert_eq!(parse_http_status("HTTP/1.0 404 Not Found\r\n").unwrap(), 404);
        assert_eq!(parse_http_status("HTTP/1.1 503 Service Unavailable").unwrap(), 503);
    }

    #[test]
    fn test_parse_http_status_invalid() {
        assert!(parse_http_status("invalid").is_err());
        assert!(parse_http_status("").is_err());
    }

    #[tokio::test]
    async fn test_tcp_health_check_success() {
        // Start a test server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept in background
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        // Health check should pass
        let result = tcp_health_check(addr, Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tcp_health_check_refused() {
        // Use a port that's not listening
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let result = tcp_health_check(addr, Duration::from_secs(1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tcp_health_check_timeout() {
        // Use a non-routable address to trigger timeout
        let addr: SocketAddr = "10.255.255.1:12345".parse().unwrap();

        let result = tcp_health_check(addr, Duration::from_millis(100)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timeout"));
    }
}
