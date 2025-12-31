//! Frontend listener implementation.
//!
//! Accepts incoming connections and dispatches them to the appropriate handler.

use crate::backend::BackendRouter;
use crate::config::{FrontendConfig, HttpConfig, Protocol, TcpConfig};
use crate::metrics::MetricsCollector;
use crate::proxy::{handle_tcp_proxy, proxy_request, HttpProxyConfig, ProxyContext, TcpProxyError};
use crate::util::RequestId;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, warn};

/// Default connect timeout if not specified in config.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Frontend listener that accepts and handles connections.
pub struct FrontendListener {
    /// Frontend configuration.
    config: FrontendConfig,
    /// Backend router for selecting upstream servers.
    router: Arc<BackendRouter>,
    /// TCP listener.
    listener: TcpListener,
    /// Metrics collector.
    metrics: MetricsCollector,
}

impl FrontendListener {
    /// Create a new frontend listener.
    pub async fn bind(
        config: FrontendConfig,
        router: Arc<BackendRouter>,
        metrics: MetricsCollector,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(config.listen).await?;

        info!(
            name = %config.name,
            listen = %config.listen,
            protocol = ?config.protocol,
            backend = %config.backend,
            "frontend listener bound"
        );

        Ok(Self {
            config,
            router,
            listener,
            metrics,
        })
    }

    /// Run the listener, accepting connections until shutdown.
    #[instrument(skip_all, fields(frontend = %self.config.name))]
    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) {
        info!("frontend listener starting");

        loop {
            tokio::select! {
                // Accept new connections
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            self.handle_connection(stream, addr);
                        }
                        Err(e) => {
                            error!(error = %e, "failed to accept connection");
                        }
                    }
                }

                // Handle shutdown signal
                _ = shutdown.recv() => {
                    info!("frontend listener shutting down");
                    break;
                }
            }
        }
    }

    /// Handle an incoming connection.
    fn handle_connection(&self, stream: TcpStream, client_addr: SocketAddr) {
        // Set TCP_NODELAY on client connection
        if let Err(e) = stream.set_nodelay(true) {
            warn!(error = %e, "failed to set TCP_NODELAY on client connection");
        }

        let frontend_name = self.config.name.clone();
        let backend_name = self.config.backend.clone();
        let protocol = self.config.protocol.clone();
        let router = Arc::clone(&self.router);
        let tcp_config = self.config.tcp.clone();
        let http_config = self.config.http.clone();
        let metrics = self.metrics.clone();
        let request_id = RequestId::short();

        // Track connection opened
        metrics.connection_opened(&frontend_name, &backend_name);

        // Spawn a task to handle this connection
        tokio::spawn(async move {
            let start_time = Instant::now();

            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = match protocol {
                Protocol::Tcp => {
                    handle_tcp_connection(
                        stream,
                        client_addr,
                        &frontend_name,
                        &backend_name,
                        &router,
                        tcp_config,
                        &metrics,
                        &request_id,
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                }
                Protocol::Http => {
                    handle_http_connection(
                        stream,
                        client_addr,
                        &frontend_name,
                        &backend_name,
                        &router,
                        http_config,
                        &metrics,
                        &request_id,
                    )
                    .await
                }
            };

            // Track connection closed
            metrics.connection_closed(&frontend_name, &backend_name);

            let duration = start_time.elapsed();

            if let Err(e) = result {
                warn!(
                    frontend = frontend_name,
                    client = %client_addr,
                    request_id = %request_id,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "connection handling failed"
                );
            } else {
                debug!(
                    frontend = frontend_name,
                    client = %client_addr,
                    request_id = %request_id,
                    duration_ms = duration.as_millis(),
                    "connection completed"
                );
            }
        });
    }
}

/// Handle a TCP connection.
#[allow(clippy::too_many_arguments)]
async fn handle_tcp_connection(
    client_stream: TcpStream,
    client_addr: SocketAddr,
    frontend_name: &str,
    backend_name: &str,
    router: &BackendRouter,
    tcp_config: Option<TcpConfig>,
    metrics: &MetricsCollector,
    request_id: &RequestId,
) -> Result<(), TcpProxyError> {
    // Select a backend server
    let backend_addr = router
        .select(backend_name, Some(client_addr))
        .ok_or_else(|| {
            TcpProxyError::BackendConnectError(
                "0.0.0.0:0".parse().unwrap(),
                std::io::Error::new(std::io::ErrorKind::NotFound, "no backend servers available"),
            )
        })?;

    info!(
        request_id = %request_id,
        client = %client_addr,
        backend = %backend_addr,
        "TCP proxy session starting"
    );

    // Get connect timeout
    let connect_timeout = tcp_config
        .map(|c| c.connect_timeout)
        .unwrap_or(DEFAULT_CONNECT_TIMEOUT);

    // Notify router of connection start
    router.on_connect(backend_name, backend_addr);

    // Handle the proxy
    let start = Instant::now();
    let result = handle_tcp_proxy(client_stream, client_addr, backend_addr, connect_timeout).await;
    let duration = start.elapsed();

    // Record metrics
    if let Ok(ref proxy_result) = result {
        metrics.record_tcp_session(
            frontend_name,
            backend_name,
            proxy_result.bytes_to_backend,
            proxy_result.bytes_to_client,
            duration,
        );

        info!(
            request_id = %request_id,
            client = %client_addr,
            backend = %backend_addr,
            bytes_to_backend = proxy_result.bytes_to_backend,
            bytes_to_client = proxy_result.bytes_to_client,
            duration_ms = duration.as_millis(),
            "TCP proxy session completed"
        );
    }

    // Notify router of connection end
    router.on_disconnect(backend_name, backend_addr);

    result.map(|_| ())
}

/// Handle an HTTP connection.
#[allow(clippy::too_many_arguments)]
async fn handle_http_connection(
    client_stream: TcpStream,
    client_addr: SocketAddr,
    frontend_name: &str,
    backend_name: &str,
    router: &BackendRouter,
    http_config: Option<HttpConfig>,
    metrics: &MetricsCollector,
    request_id: &RequestId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Select a backend server
    let backend_addr = router
        .select(backend_name, Some(client_addr))
        .ok_or("no backend servers available")?;

    info!(
        request_id = %request_id,
        client = %client_addr,
        backend = %backend_addr,
        "HTTP connection started"
    );

    // Build the proxy config from frontend HTTP config
    let proxy_config = HttpProxyConfig {
        request_headers: http_config
            .as_ref()
            .map(|c| c.request_headers.clone())
            .unwrap_or_default(),
        response_headers: http_config
            .as_ref()
            .map(|c| c.response_headers.clone())
            .unwrap_or_default(),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
    };

    // Create the proxy context with metrics
    let ctx = ProxyContext {
        client_addr,
        backend_addr,
        frontend_name: frontend_name.to_string(),
        backend_name: backend_name.to_string(),
        config: proxy_config,
        metrics: metrics.clone(),
        connection_request_id: request_id.as_str().to_string(),
    };

    // Notify router of connection start
    router.on_connect(backend_name, backend_addr);

    // Wrap the TCP stream for hyper
    let io = TokioIo::new(client_stream);

    // Create the HTTP service that proxies requests
    let service = service_fn(move |req| {
        let ctx = ctx.clone();
        async move { proxy_request(req, ctx).await }
    });

    // Serve HTTP/1.1 with keep-alive support
    let result = http1::Builder::new()
        .keep_alive(true)
        .serve_connection(io, service)
        .await;

    // Notify router of connection end
    router.on_disconnect(backend_name, backend_addr);

    result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Algorithm, BackendConfig, ServerConfig};

    #[tokio::test]
    async fn test_frontend_listener_bind() {
        let config = FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:0".parse().unwrap(),
            protocol: Protocol::Tcp,
            backend: "test-backend".to_string(),
            algorithm: Algorithm::RoundRobin,
            http: None,
            tcp: None,
        };

        let backends = vec![BackendConfig {
            name: "test-backend".to_string(),
            servers: vec![ServerConfig {
                address: "127.0.0.1:9000".parse().unwrap(),
                weight: 1,
            }],
            health_check: None,
        }];

        let frontends = vec![config.clone()];
        let router = Arc::new(BackendRouter::new(&backends, &frontends));
        let metrics = MetricsCollector::new();

        let listener = FrontendListener::bind(config, router, metrics).await;
        assert!(listener.is_ok());
    }
}
