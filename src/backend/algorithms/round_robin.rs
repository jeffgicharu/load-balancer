//! Round-robin load balancing algorithm.
//!
//! To be implemented in Phase 3.

use super::LoadBalancer;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Round-robin load balancer.
///
/// Distributes requests evenly across all servers in order.
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    /// Create a new round-robin load balancer.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for RoundRobin {
    fn select(&self, servers: &[SocketAddr], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        if servers.is_empty() {
            return None;
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % servers.len();
        Some(servers[idx])
    }
}
