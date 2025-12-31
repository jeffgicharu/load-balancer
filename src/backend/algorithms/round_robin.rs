//! Round-robin load balancing algorithm.

use super::{LoadBalancer, ServerInfo};
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
    fn select(&self, servers: &[ServerInfo], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        if servers.is_empty() {
            return None;
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % servers.len();
        Some(servers[idx].address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_servers() -> Vec<ServerInfo> {
        vec![
            ServerInfo {
                address: "127.0.0.1:8001".parse().unwrap(),
                weight: 1,
            },
            ServerInfo {
                address: "127.0.0.1:8002".parse().unwrap(),
                weight: 1,
            },
            ServerInfo {
                address: "127.0.0.1:8003".parse().unwrap(),
                weight: 1,
            },
        ]
    }

    #[test]
    fn test_round_robin_cycles() {
        let rr = RoundRobin::new();
        let servers = test_servers();

        let s1 = rr.select(&servers, None).unwrap();
        let s2 = rr.select(&servers, None).unwrap();
        let s3 = rr.select(&servers, None).unwrap();
        let s4 = rr.select(&servers, None).unwrap();

        assert_eq!(s1, servers[0].address);
        assert_eq!(s2, servers[1].address);
        assert_eq!(s3, servers[2].address);
        assert_eq!(s4, servers[0].address); // Cycles back
    }

    #[test]
    fn test_round_robin_empty() {
        let rr = RoundRobin::new();
        assert!(rr.select(&[], None).is_none());
    }
}
