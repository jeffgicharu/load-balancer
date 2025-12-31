//! Least-connections load balancing algorithm.
//!
//! To be implemented in Phase 3.

use super::LoadBalancer;
use std::net::SocketAddr;

/// Least-connections load balancer.
///
/// Sends requests to the server with the fewest active connections.
pub struct LeastConnections;

impl LeastConnections {
    /// Create a new least-connections load balancer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastConnections {
    fn select(&self, servers: &[SocketAddr], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        // TODO: Implement least-connections selection
        servers.first().copied()
    }
}
