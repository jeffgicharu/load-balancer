//! Backend router for selecting upstream servers.

use crate::backend::algorithms::{IpHash, LeastConnections, LoadBalancer, RoundRobin, ServerInfo, Weighted};
use crate::config::{Algorithm, BackendConfig, FrontendConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, warn};

/// Routes requests to backend servers based on configured algorithm.
pub struct BackendRouter {
    /// Map of backend name to backend info.
    backends: HashMap<String, BackendInfo>,
}

/// Information about a backend pool.
struct BackendInfo {
    /// List of servers with their weights.
    servers: Vec<ServerInfo>,
    /// The load balancer algorithm.
    algorithm: Arc<dyn LoadBalancer>,
}

impl BackendRouter {
    /// Create a new backend router from configuration.
    pub fn new(backends: &[BackendConfig], frontends: &[FrontendConfig]) -> Self {
        let mut backend_map = HashMap::new();

        // Build a map of frontend -> algorithm
        let frontend_algorithms: HashMap<&str, Algorithm> = frontends
            .iter()
            .map(|f| (f.backend.as_str(), f.algorithm.clone()))
            .collect();

        for backend in backends {
            let servers: Vec<ServerInfo> = backend
                .servers
                .iter()
                .map(|s| ServerInfo {
                    address: s.address,
                    weight: s.weight,
                })
                .collect();

            // Get the algorithm for this backend (from the frontend that uses it)
            let algorithm = frontend_algorithms
                .get(backend.name.as_str())
                .cloned()
                .unwrap_or(Algorithm::RoundRobin);

            let lb: Arc<dyn LoadBalancer> = match algorithm {
                Algorithm::RoundRobin => Arc::new(RoundRobin::new()),
                Algorithm::Weighted => Arc::new(Weighted::new()),
                Algorithm::LeastConnections => Arc::new(LeastConnections::new()),
                Algorithm::IpHash => Arc::new(IpHash::new()),
            };

            backend_map.insert(
                backend.name.clone(),
                BackendInfo {
                    servers,
                    algorithm: lb,
                },
            );
        }

        Self {
            backends: backend_map,
        }
    }

    /// Select a backend server for the given backend name.
    ///
    /// # Arguments
    ///
    /// * `backend_name` - Name of the backend pool
    /// * `client_addr` - Client's address (used for IP hash)
    ///
    /// # Returns
    ///
    /// The selected server address, or None if no servers available.
    pub fn select(
        &self,
        backend_name: &str,
        client_addr: Option<SocketAddr>,
    ) -> Option<SocketAddr> {
        let backend = self.backends.get(backend_name)?;

        if backend.servers.is_empty() {
            warn!(backend = backend_name, "no servers configured for backend");
            return None;
        }

        let selected = backend.algorithm.select(&backend.servers, client_addr);

        if let Some(addr) = selected {
            debug!(backend = backend_name, server = %addr, "selected backend server");
        } else {
            warn!(backend = backend_name, "no healthy servers available");
        }

        selected
    }

    /// Get all servers for a backend.
    pub fn get_servers(&self, backend_name: &str) -> Option<Vec<SocketAddr>> {
        self.backends
            .get(backend_name)
            .map(|b| b.servers.iter().map(|s| s.address).collect())
    }

    /// Notify that a connection was established to a server.
    pub fn on_connect(&self, backend_name: &str, server: SocketAddr) {
        if let Some(backend) = self.backends.get(backend_name) {
            backend.algorithm.on_connect(server);
        }
    }

    /// Notify that a connection was closed to a server.
    pub fn on_disconnect(&self, backend_name: &str, server: SocketAddr) {
        if let Some(backend) = self.backends.get(backend_name) {
            backend.algorithm.on_disconnect(server);
        }
    }

    /// Get connection count for a server (for metrics/debugging).
    pub fn connection_count(&self, backend_name: &str, server: SocketAddr) -> u32 {
        self.backends
            .get(backend_name)
            .map(|b| b.algorithm.connection_count(server))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;

    fn test_backends() -> Vec<BackendConfig> {
        vec![BackendConfig {
            name: "test-backend".to_string(),
            servers: vec![
                ServerConfig {
                    address: "127.0.0.1:9001".parse().unwrap(),
                    weight: 1,
                },
                ServerConfig {
                    address: "127.0.0.1:9002".parse().unwrap(),
                    weight: 1,
                },
            ],
            health_check: None,
        }]
    }

    fn test_frontends() -> Vec<FrontendConfig> {
        vec![FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:8080".parse().unwrap(),
            protocol: crate::config::Protocol::Tcp,
            backend: "test-backend".to_string(),
            algorithm: Algorithm::RoundRobin,
            http: None,
            tcp: None,
        }]
    }

    #[test]
    fn test_round_robin_selection() {
        let router = BackendRouter::new(&test_backends(), &test_frontends());

        let s1 = router.select("test-backend", None).unwrap();
        let s2 = router.select("test-backend", None).unwrap();
        let s3 = router.select("test-backend", None).unwrap();

        // Should cycle through servers
        assert_ne!(s1, s2);
        assert_eq!(s1, s3); // Back to first server
    }

    #[test]
    fn test_nonexistent_backend() {
        let router = BackendRouter::new(&test_backends(), &test_frontends());
        assert!(router.select("nonexistent", None).is_none());
    }

    #[test]
    fn test_weighted_selection() {
        let backends = vec![BackendConfig {
            name: "weighted-backend".to_string(),
            servers: vec![
                ServerConfig {
                    address: "127.0.0.1:9001".parse().unwrap(),
                    weight: 3,
                },
                ServerConfig {
                    address: "127.0.0.1:9002".parse().unwrap(),
                    weight: 1,
                },
            ],
            health_check: None,
        }];

        let frontends = vec![FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:8080".parse().unwrap(),
            protocol: crate::config::Protocol::Tcp,
            backend: "weighted-backend".to_string(),
            algorithm: Algorithm::Weighted,
            http: None,
            tcp: None,
        }];

        let router = BackendRouter::new(&backends, &frontends);

        let mut s1_count = 0;
        let mut s2_count = 0;

        for _ in 0..40 {
            let selected = router.select("weighted-backend", None).unwrap();
            if selected == "127.0.0.1:9001".parse().unwrap() {
                s1_count += 1;
            } else {
                s2_count += 1;
            }
        }

        // With 3:1 weights, s1 should get 30, s2 should get 10
        assert_eq!(s1_count, 30);
        assert_eq!(s2_count, 10);
    }

    #[test]
    fn test_least_connections_selection() {
        let backends = vec![BackendConfig {
            name: "lc-backend".to_string(),
            servers: vec![
                ServerConfig {
                    address: "127.0.0.1:9001".parse().unwrap(),
                    weight: 1,
                },
                ServerConfig {
                    address: "127.0.0.1:9002".parse().unwrap(),
                    weight: 1,
                },
            ],
            health_check: None,
        }];

        let frontends = vec![FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:8080".parse().unwrap(),
            protocol: crate::config::Protocol::Tcp,
            backend: "lc-backend".to_string(),
            algorithm: Algorithm::LeastConnections,
            http: None,
            tcp: None,
        }];

        let router = BackendRouter::new(&backends, &frontends);

        // Add connections to first server
        let s1: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        router.on_connect("lc-backend", s1);
        router.on_connect("lc-backend", s1);

        // Should select second server (fewer connections)
        let selected = router.select("lc-backend", None).unwrap();
        assert_eq!(selected, "127.0.0.1:9002".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_ip_hash_consistency() {
        let backends = vec![BackendConfig {
            name: "ip-backend".to_string(),
            servers: vec![
                ServerConfig {
                    address: "127.0.0.1:9001".parse().unwrap(),
                    weight: 1,
                },
                ServerConfig {
                    address: "127.0.0.1:9002".parse().unwrap(),
                    weight: 1,
                },
            ],
            health_check: None,
        }];

        let frontends = vec![FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:8080".parse().unwrap(),
            protocol: crate::config::Protocol::Tcp,
            backend: "ip-backend".to_string(),
            algorithm: Algorithm::IpHash,
            http: None,
            tcp: None,
        }];

        let router = BackendRouter::new(&backends, &frontends);

        let client: SocketAddr = "192.168.1.100:12345".parse().unwrap();

        // Same client should always get same server
        let s1 = router.select("ip-backend", Some(client)).unwrap();
        let s2 = router.select("ip-backend", Some(client)).unwrap();
        let s3 = router.select("ip-backend", Some(client)).unwrap();

        assert_eq!(s1, s2);
        assert_eq!(s2, s3);
    }
}
