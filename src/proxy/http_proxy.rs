//! HTTP proxy implementation.
//!
//! Provides HTTP/1.1 proxying with header manipulation.

use crate::metrics::MetricsCollector;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tracing::{debug, error, info, instrument, warn};

/// HTTP proxy configuration.
#[derive(Clone)]
pub struct HttpProxyConfig {
    /// Headers to add to requests.
    pub request_headers: HashMap<String, String>,
    /// Headers to add to responses.
    pub response_headers: HashMap<String, String>,
    /// Connect timeout for backend.
    pub connect_timeout: Duration,
}

impl Default for HttpProxyConfig {
    fn default() -> Self {
        Self {
            request_headers: HashMap::new(),
            response_headers: HashMap::new(),
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// Context for an HTTP proxy request.
#[derive(Clone)]
pub struct ProxyContext {
    /// Client's address.
    pub client_addr: SocketAddr,
    /// Backend server address.
    pub backend_addr: SocketAddr,
    /// Frontend name for metrics.
    pub frontend_name: String,
    /// Backend name for logging and metrics.
    pub backend_name: String,
    /// Proxy configuration.
    pub config: HttpProxyConfig,
    /// Metrics collector.
    pub metrics: MetricsCollector,
    /// Connection-level request ID.
    pub connection_request_id: String,
}

/// HTTP proxy error.
#[derive(Debug, thiserror::Error)]
pub enum HttpProxyError {
    #[error("failed to connect to backend: {0}")]
    BackendConnectError(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    HttpError(#[from] hyper::Error),

    #[error("no backend available")]
    NoBackendAvailable,
}

/// Proxy a single HTTP request to the backend.
#[instrument(skip_all, fields(
    method = %req.method(),
    uri = %req.uri(),
    client = %ctx.client_addr,
    backend = %ctx.backend_addr
))]
pub async fn proxy_request(
    mut req: Request<Incoming>,
    ctx: ProxyContext,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
    let start_time = Instant::now();
    let method = req.method().to_string();
    let uri = req.uri().to_string();

    debug!(
        connection_id = %ctx.connection_request_id,
        "proxying HTTP request"
    );

    // Add request headers
    add_request_headers(&mut req, &ctx);

    // Connect to backend
    let backend_stream = match TcpStream::connect(ctx.backend_addr).await {
        Ok(stream) => {
            let _ = stream.set_nodelay(true);
            stream
        }
        Err(e) => {
            error!(
                connection_id = %ctx.connection_request_id,
                error = %e,
                "failed to connect to backend"
            );
            let duration = start_time.elapsed();
            ctx.metrics.record_request(
                &ctx.frontend_name,
                &ctx.backend_name,
                &method,
                502,
                duration,
            );
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "Failed to connect to backend",
            ));
        }
    };

    let io = TokioIo::new(backend_stream);

    // Create HTTP client connection
    let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
        Ok(result) => result,
        Err(e) => {
            error!(
                connection_id = %ctx.connection_request_id,
                error = %e,
                "backend handshake failed"
            );
            let duration = start_time.elapsed();
            ctx.metrics.record_request(
                &ctx.frontend_name,
                &ctx.backend_name,
                &method,
                502,
                duration,
            );
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "Backend handshake failed",
            ));
        }
    };

    // Spawn connection driver
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            warn!(error = %e, "backend connection error");
        }
    });

    // Modify the request URI to be relative (required for proxying)
    let req_uri = req.uri().clone();
    let path_and_query = req_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    *req.uri_mut() = path_and_query.parse().unwrap_or_else(|_| "/".parse().unwrap());

    // Send request to backend
    let backend_response = match sender.send_request(req).await {
        Ok(resp) => resp,
        Err(e) => {
            error!(
                connection_id = %ctx.connection_request_id,
                error = %e,
                "failed to send request to backend"
            );
            let duration = start_time.elapsed();
            ctx.metrics.record_request(
                &ctx.frontend_name,
                &ctx.backend_name,
                &method,
                502,
                duration,
            );
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "Failed to send request to backend",
            ));
        }
    };

    // Convert the response
    let (mut parts, body) = backend_response.into_parts();
    let status_code = parts.status.as_u16();

    // Add response headers
    add_response_headers(&mut parts.headers, &ctx);

    // Build the response with boxed body
    let boxed_body = body.map_err(|e| e).boxed();
    let response = Response::from_parts(parts, boxed_body);

    // Record metrics
    let duration = start_time.elapsed();
    ctx.metrics.record_request(
        &ctx.frontend_name,
        &ctx.backend_name,
        &method,
        status_code,
        duration,
    );

    info!(
        connection_id = %ctx.connection_request_id,
        method = %method,
        uri = %uri,
        status = status_code,
        duration_ms = duration.as_millis(),
        "proxied request completed"
    );

    Ok(response)
}

/// Add headers to the request being sent to the backend.
fn add_request_headers(req: &mut Request<Incoming>, ctx: &ProxyContext) {
    let headers = req.headers_mut();

    // Add X-Forwarded-For
    let forwarded_for = ctx.client_addr.ip().to_string();
    if let Ok(value) = forwarded_for.parse() {
        headers.insert("x-forwarded-for", value);
    }

    // Add X-Real-IP
    if let Ok(value) = ctx.client_addr.ip().to_string().parse() {
        headers.insert("x-real-ip", value);
    }

    // Add custom headers from config (with variable substitution)
    for (name, value) in &ctx.config.request_headers {
        let value = substitute_variables(value, ctx);
        if let (Ok(name), Ok(value)) = (
            name.parse::<hyper::header::HeaderName>(),
            value.parse::<hyper::header::HeaderValue>(),
        ) {
            headers.insert(name, value);
        }
    }

    // Ensure Host header is set correctly for the backend
    // (keep the original Host header for virtual hosting)
}

/// Add headers to the response being sent to the client.
fn add_response_headers(headers: &mut hyper::HeaderMap, ctx: &ProxyContext) {
    // Add X-Served-By
    let served_by = format!("{}:{}", ctx.backend_name, ctx.backend_addr);
    if let Ok(value) = served_by.parse() {
        headers.insert("x-served-by", value);
    }

    // Add custom headers from config
    for (name, value) in &ctx.config.response_headers {
        let value = substitute_variables(value, ctx);
        if let (Ok(name), Ok(value)) = (
            name.parse::<hyper::header::HeaderName>(),
            value.parse::<hyper::header::HeaderValue>(),
        ) {
            headers.insert(name, value);
        }
    }
}

/// Substitute variables in header values.
fn substitute_variables(value: &str, ctx: &ProxyContext) -> String {
    value
        .replace("$client_ip", &ctx.client_addr.ip().to_string())
        .replace("$client_port", &ctx.client_addr.port().to_string())
        .replace("$backend_name", &ctx.backend_name)
        .replace("$backend_addr", &ctx.backend_addr.to_string())
}

/// Create an error response.
fn error_response(status: StatusCode, message: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = Full::new(Bytes::from(format!("{}: {}\n", status, message)))
        .map_err(|never| match never {})
        .boxed();

    Response::builder()
        .status(status)
        .header("content-type", "text/plain")
        .body(body)
        .unwrap()
}

/// Placeholder for HTTP proxy struct (will be used later).
pub struct HttpProxy;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> ProxyContext {
        ProxyContext {
            client_addr: "192.168.1.100:12345".parse().unwrap(),
            backend_addr: "10.0.0.1:8080".parse().unwrap(),
            frontend_name: "test-frontend".to_string(),
            backend_name: "web-servers".to_string(),
            config: HttpProxyConfig::default(),
            metrics: MetricsCollector::new(),
            connection_request_id: "test-request-123".to_string(),
        }
    }

    #[test]
    fn test_substitute_variables() {
        let ctx = test_context();

        assert_eq!(
            substitute_variables("$client_ip", &ctx),
            "192.168.1.100"
        );
        assert_eq!(
            substitute_variables("$backend_name", &ctx),
            "web-servers"
        );
        assert_eq!(
            substitute_variables("client=$client_ip:$client_port", &ctx),
            "client=192.168.1.100:12345"
        );
    }

    #[test]
    fn test_error_response() {
        let resp = error_response(StatusCode::BAD_GATEWAY, "test error");
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
