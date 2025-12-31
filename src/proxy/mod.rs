//! Proxy implementations for TCP and HTTP.

mod http_proxy;
mod tcp_proxy;

pub use http_proxy::{proxy_request, HttpProxy, HttpProxyConfig, HttpProxyError, ProxyContext};
pub use tcp_proxy::{
    connect_to_backend, handle_tcp_proxy, proxy_bidirectional, ProxyResult, TcpProxyError,
};
