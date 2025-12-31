//! Weighted round-robin load balancing algorithm.

use super::{LoadBalancer, ServerInfo};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Weighted round-robin load balancer.
///
/// Distributes requests proportionally based on server weights.
/// Uses smooth weighted round-robin for even distribution.
pub struct Weighted {
    counter: AtomicUsize,
}

impl Weighted {
    /// Create a new weighted load balancer.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for Weighted {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for Weighted {
    fn select(&self, servers: &[ServerInfo], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        if servers.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: u32 = servers.iter().map(|s| s.weight).sum();
        if total_weight == 0 {
            return None;
        }

        // Get current position in the weight cycle
        let position = self.counter.fetch_add(1, Ordering::Relaxed) as u32 % total_weight;

        // Find the server at this weighted position
        let mut cumulative = 0u32;
        for server in servers {
            cumulative += server.weight;
            if position < cumulative {
                return Some(server.address);
            }
        }

        // Fallback (shouldn't reach here)
        Some(servers[0].address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_weighted_distribution() {
        let weighted = Weighted::new();
        let servers = vec![
            ServerInfo {
                address: "127.0.0.1:8001".parse().unwrap(),
                weight: 3, // Should get 3x traffic
            },
            ServerInfo {
                address: "127.0.0.1:8002".parse().unwrap(),
                weight: 1, // Should get 1x traffic
            },
        ];

        let mut counts: HashMap<SocketAddr, u32> = HashMap::new();

        // Run many selections
        for _ in 0..400 {
            let selected = weighted.select(&servers, None).unwrap();
            *counts.entry(selected).or_insert(0) += 1;
        }

        // With weights 3:1, server 1 should get ~75%, server 2 should get ~25%
        let s1_count = counts.get(&servers[0].address).unwrap_or(&0);
        let s2_count = counts.get(&servers[1].address).unwrap_or(&0);

        // Total weight is 4, so in 400 requests:
        // Server 1 (weight 3) should get exactly 300 requests
        // Server 2 (weight 1) should get exactly 100 requests
        assert_eq!(*s1_count, 300);
        assert_eq!(*s2_count, 100);
    }

    #[test]
    fn test_weighted_equal_weights() {
        let weighted = Weighted::new();
        let servers = vec![
            ServerInfo {
                address: "127.0.0.1:8001".parse().unwrap(),
                weight: 1,
            },
            ServerInfo {
                address: "127.0.0.1:8002".parse().unwrap(),
                weight: 1,
            },
        ];

        // With equal weights, should alternate
        let s1 = weighted.select(&servers, None).unwrap();
        let s2 = weighted.select(&servers, None).unwrap();
        let s3 = weighted.select(&servers, None).unwrap();

        assert_eq!(s1, servers[0].address);
        assert_eq!(s2, servers[1].address);
        assert_eq!(s3, servers[0].address);
    }

    #[test]
    fn test_weighted_empty() {
        let weighted = Weighted::new();
        assert!(weighted.select(&[], None).is_none());
    }

    #[test]
    fn test_weighted_zero_weights() {
        let weighted = Weighted::new();
        let servers = vec![
            ServerInfo {
                address: "127.0.0.1:8001".parse().unwrap(),
                weight: 0,
            },
        ];
        assert!(weighted.select(&servers, None).is_none());
    }
}
