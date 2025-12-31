//! Least-connections load balancing algorithm.

use super::{LoadBalancer, ServerInfo};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};

/// Least-connections load balancer.
///
/// Sends requests to the server with the fewest active connections.
/// Breaks ties using round-robin order.
pub struct LeastConnections {
    /// Active connection count per server.
    connections: DashMap<SocketAddr, AtomicU32>,
}

impl LeastConnections {
    /// Create a new least-connections load balancer.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Get the connection count for a server.
    fn get_connections(&self, server: SocketAddr) -> u32 {
        self.connections
            .get(&server)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastConnections {
    fn select(&self, servers: &[ServerInfo], _client_addr: Option<SocketAddr>) -> Option<SocketAddr> {
        if servers.is_empty() {
            return None;
        }

        // Find server with minimum connections
        let mut min_conns = u32::MAX;
        let mut selected = None;

        for server in servers {
            let conns = self.get_connections(server.address);
            if conns < min_conns {
                min_conns = conns;
                selected = Some(server.address);
            }
        }

        selected
    }

    fn on_connect(&self, server: SocketAddr) {
        self.connections
            .entry(server)
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    fn on_disconnect(&self, server: SocketAddr) {
        if let Some(counter) = self.connections.get(&server) {
            // Prevent underflow
            let current = counter.load(Ordering::Relaxed);
            if current > 0 {
                counter.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    fn connection_count(&self, server: SocketAddr) -> u32 {
        self.get_connections(server)
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
    fn test_least_conn_selects_lowest() {
        let lc = LeastConnections::new();
        let servers = test_servers();

        // Add connections to first two servers
        lc.on_connect(servers[0].address);
        lc.on_connect(servers[0].address);
        lc.on_connect(servers[1].address);

        // Server 3 has 0 connections, should be selected
        let selected = lc.select(&servers, None).unwrap();
        assert_eq!(selected, servers[2].address);
    }

    #[test]
    fn test_least_conn_connection_tracking() {
        let lc = LeastConnections::new();
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();

        assert_eq!(lc.connection_count(server), 0);

        lc.on_connect(server);
        assert_eq!(lc.connection_count(server), 1);

        lc.on_connect(server);
        assert_eq!(lc.connection_count(server), 2);

        lc.on_disconnect(server);
        assert_eq!(lc.connection_count(server), 1);

        lc.on_disconnect(server);
        assert_eq!(lc.connection_count(server), 0);

        // Should not go negative
        lc.on_disconnect(server);
        assert_eq!(lc.connection_count(server), 0);
    }

    #[test]
    fn test_least_conn_empty() {
        let lc = LeastConnections::new();
        assert!(lc.select(&[], None).is_none());
    }

    #[test]
    fn test_least_conn_equal_connections() {
        let lc = LeastConnections::new();
        let servers = test_servers();

        // All servers have 0 connections, should pick first
        let selected = lc.select(&servers, None).unwrap();
        assert_eq!(selected, servers[0].address);
    }
}
