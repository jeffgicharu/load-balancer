//! IP hash load balancing algorithm.
//!
//! To be implemented in Phase 3.

use super::LoadBalancer;
use std::net::SocketAddr;

/// IP hash load balancer.
///
/// Consistently routes requests from the same client IP to the same server.
pub struct IpHash;

impl IpHash {
    /// Create a new IP hash load balancer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for IpHash {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for IpHash {
    fn select(&self, servers: &[SocketAddr], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        // TODO: Implement IP hash selection
        servers.first().copied()
    }
}
