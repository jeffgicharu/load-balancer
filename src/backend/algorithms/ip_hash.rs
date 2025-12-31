//! IP hash load balancing algorithm.

use super::{LoadBalancer, ServerInfo};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

/// IP hash load balancer.
///
/// Consistently routes requests from the same client IP to the same server.
/// Uses a hash of the client's IP address to determine server selection.
pub struct IpHash;

impl IpHash {
    /// Create a new IP hash load balancer.
    pub fn new() -> Self {
        Self
    }

    /// Hash a client address to get a consistent index.
    fn hash_client(&self, client_addr: SocketAddr) -> u64 {
        let mut hasher = DefaultHasher::new();
        // Only hash the IP, not the port (port changes between connections)
        client_addr.ip().hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for IpHash {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for IpHash {
    fn select(&self, servers: &[ServerInfo], client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        if servers.is_empty() {
            return None;
        }

        let idx = match client_addr {
            Some(addr) => {
                let hash = self.hash_client(addr);
                (hash as usize) % servers.len()
            }
            None => {
                // No client address, fall back to first server
                0
            }
        };

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
    fn test_ip_hash_consistency() {
        let ip_hash = IpHash::new();
        let servers = test_servers();

        let client: SocketAddr = "192.168.1.100:12345".parse().unwrap();

        // Same client should always get same server
        let s1 = ip_hash.select(&servers, Some(client)).unwrap();
        let s2 = ip_hash.select(&servers, Some(client)).unwrap();
        let s3 = ip_hash.select(&servers, Some(client)).unwrap();

        assert_eq!(s1, s2);
        assert_eq!(s2, s3);
    }

    #[test]
    fn test_ip_hash_different_ports_same_server() {
        let ip_hash = IpHash::new();
        let servers = test_servers();

        // Same IP, different ports should get same server
        let client1: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let client2: SocketAddr = "192.168.1.100:54321".parse().unwrap();

        let s1 = ip_hash.select(&servers, Some(client1)).unwrap();
        let s2 = ip_hash.select(&servers, Some(client2)).unwrap();

        assert_eq!(s1, s2);
    }

    #[test]
    fn test_ip_hash_different_ips() {
        let ip_hash = IpHash::new();
        let servers = test_servers();

        let client1: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let client2: SocketAddr = "192.168.1.101:12345".parse().unwrap();

        let s1 = ip_hash.select(&servers, Some(client1)).unwrap();
        let s2 = ip_hash.select(&servers, Some(client2)).unwrap();

        // Different IPs might get same or different server (depends on hash)
        // But each should be consistent with itself
        let s1_again = ip_hash.select(&servers, Some(client1)).unwrap();
        let s2_again = ip_hash.select(&servers, Some(client2)).unwrap();

        assert_eq!(s1, s1_again);
        assert_eq!(s2, s2_again);
    }

    #[test]
    fn test_ip_hash_no_client() {
        let ip_hash = IpHash::new();
        let servers = test_servers();

        // No client address should fall back to first server
        let selected = ip_hash.select(&servers, None).unwrap();
        assert_eq!(selected, servers[0].address);
    }

    #[test]
    fn test_ip_hash_empty() {
        let ip_hash = IpHash::new();
        assert!(ip_hash.select(&[], None).is_none());
    }
}
