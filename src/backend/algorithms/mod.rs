//! Load balancing algorithms.

mod ip_hash;
mod least_conn;
mod round_robin;
mod weighted;

pub use ip_hash::IpHash;
pub use least_conn::LeastConnections;
pub use round_robin::RoundRobin;
pub use weighted::Weighted;

use std::net::SocketAddr;

/// Trait for load balancing algorithms.
pub trait LoadBalancer: Send + Sync {
    /// Select the next backend server.
    ///
    /// # Arguments
    ///
    /// * `servers` - Available healthy servers
    /// * `client_addr` - Client's address (for IP hash)
    ///
    /// # Returns
    ///
    /// The selected server address, or None if no servers available.
    fn select(&self, servers: &[SocketAddr], client_addr: Option<SocketAddr>) -> Option<SocketAddr>;

    /// Notify that a connection to a server was established.
    fn on_connect(&self, _server: SocketAddr) {}

    /// Notify that a connection to a server was closed.
    fn on_disconnect(&self, _server: SocketAddr) {}
}
