//! TCP proxy implementation.
//!
//! Provides bidirectional data transfer between client and backend.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Result of a proxy operation.
#[derive(Debug)]
pub struct ProxyResult {
    /// Bytes sent from client to backend.
    pub bytes_to_backend: u64,
    /// Bytes sent from backend to client.
    pub bytes_to_client: u64,
}

/// TCP proxy error.
#[derive(Debug, thiserror::Error)]
pub enum TcpProxyError {
    #[error("failed to connect to backend {0}: {1}")]
    BackendConnectError(SocketAddr, io::Error),

    #[error("connection timeout to backend {0}")]
    BackendTimeout(SocketAddr),

    #[error("proxy error: {0}")]
    ProxyError(#[from] io::Error),
}

/// Connect to a backend server with timeout.
#[instrument(skip_all, fields(backend = %addr))]
pub async fn connect_to_backend(
    addr: SocketAddr,
    connect_timeout: Duration,
) -> Result<TcpStream, TcpProxyError> {
    debug!("connecting to backend");

    match timeout(connect_timeout, TcpStream::connect(addr)).await {
        Ok(Ok(stream)) => {
            debug!("connected to backend");
            // Set TCP_NODELAY for lower latency
            if let Err(e) = stream.set_nodelay(true) {
                warn!(error = %e, "failed to set TCP_NODELAY on backend connection");
            }
            Ok(stream)
        }
        Ok(Err(e)) => {
            error!(error = %e, "failed to connect to backend");
            Err(TcpProxyError::BackendConnectError(addr, e))
        }
        Err(_) => {
            error!("connection timeout");
            Err(TcpProxyError::BackendTimeout(addr))
        }
    }
}

/// Proxy data bidirectionally between two streams.
///
/// This function copies data in both directions simultaneously until
/// one side closes the connection or an error occurs.
#[instrument(skip_all)]
pub async fn proxy_bidirectional<C, B>(
    client: C,
    backend: B,
) -> Result<ProxyResult, TcpProxyError>
where
    C: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let (mut backend_read, mut backend_write) = tokio::io::split(backend);

    // Copy in both directions simultaneously
    let client_to_backend = tokio::io::copy(&mut client_read, &mut backend_write);
    let backend_to_client = tokio::io::copy(&mut backend_read, &mut client_write);

    // Wait for both directions to complete
    let (c2b_result, b2c_result) = tokio::join!(client_to_backend, backend_to_client);

    let bytes_to_backend = c2b_result.unwrap_or(0);
    let bytes_to_client = b2c_result.unwrap_or(0);

    debug!(
        bytes_to_backend = bytes_to_backend,
        bytes_to_client = bytes_to_client,
        "proxy completed"
    );

    Ok(ProxyResult {
        bytes_to_backend,
        bytes_to_client,
    })
}

/// Handle a complete TCP proxy session.
///
/// Connects to the backend and proxies data bidirectionally.
#[instrument(skip_all, fields(client = %client_addr, backend = %backend_addr))]
pub async fn handle_tcp_proxy(
    client_stream: TcpStream,
    client_addr: SocketAddr,
    backend_addr: SocketAddr,
    connect_timeout: Duration,
) -> Result<ProxyResult, TcpProxyError> {
    info!("starting TCP proxy session");

    // Connect to backend
    let backend_stream = connect_to_backend(backend_addr, connect_timeout).await?;

    // Proxy data
    let result = proxy_bidirectional(client_stream, backend_stream).await?;

    info!(
        bytes_to_backend = result.bytes_to_backend,
        bytes_to_client = result.bytes_to_client,
        "TCP proxy session completed"
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_connect_to_backend_success() {
        // Start a simple TCP server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept in background
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        // Connect should succeed
        let result = connect_to_backend(addr, Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connect_to_backend_timeout() {
        // Use a non-routable address to trigger timeout
        let addr: SocketAddr = "10.255.255.1:12345".parse().unwrap();

        let result = connect_to_backend(addr, Duration::from_millis(100)).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            TcpProxyError::BackendTimeout(_) => {}
            e => panic!("expected timeout error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_connect_to_backend_refused() {
        // Use localhost with a port that's (very likely) not listening
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let result = connect_to_backend(addr, Duration::from_secs(5)).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            TcpProxyError::BackendConnectError(_, _) => {}
            e => panic!("expected connect error, got: {:?}", e),
        }
    }
}
