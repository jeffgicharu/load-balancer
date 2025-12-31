//! Proxy implementations for TCP and HTTP.

mod http_proxy;
mod tcp_proxy;

pub use http_proxy::HttpProxy;
pub use tcp_proxy::TcpProxy;
