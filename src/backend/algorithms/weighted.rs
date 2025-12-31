//! Weighted round-robin load balancing algorithm.
//!
//! To be implemented in Phase 3.

use super::LoadBalancer;
use std::net::SocketAddr;

/// Weighted load balancer.
///
/// Distributes requests proportionally based on server weights.
pub struct Weighted;

impl Weighted {
    /// Create a new weighted load balancer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for Weighted {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for Weighted {
    fn select(&self, servers: &[SocketAddr], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        // TODO: Implement weighted selection
        servers.first().copied()
    }
}
