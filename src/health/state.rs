//! Health state management.
//!
//! Provides shared health tracking for all backend servers.

use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Shared health state for all backends.
#[derive(Debug)]
pub struct HealthState {
    /// Health information per server.
    servers: DashMap<SocketAddr, ServerHealth>,
    /// Configuration for health tracking.
    config: HealthConfig,
}

/// Configuration for health tracking.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Consecutive failures before marking unhealthy.
    pub unhealthy_threshold: u32,
    /// Consecutive successes before marking healthy.
    pub healthy_threshold: u32,
    /// Cooldown period before retrying unhealthy server.
    pub cooldown: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(30),
        }
    }
}

/// Health information for a single server.
#[derive(Debug)]
pub struct ServerHealth {
    /// Is this server currently healthy?
    healthy: AtomicBool,
    /// Consecutive failures count.
    consecutive_failures: AtomicU32,
    /// Consecutive successes count.
    consecutive_successes: AtomicU32,
    /// Active connection count.
    active_connections: AtomicU32,
    /// Unix timestamp (seconds) when server became unhealthy (0 if healthy).
    unhealthy_since: AtomicU64,
    /// Unix timestamp (seconds) of last health check.
    last_check: AtomicU64,
}

impl Default for ServerHealth {
    fn default() -> Self {
        Self {
            healthy: AtomicBool::new(true),
            consecutive_failures: AtomicU32::new(0),
            consecutive_successes: AtomicU32::new(0),
            active_connections: AtomicU32::new(0),
            unhealthy_since: AtomicU64::new(0),
            last_check: AtomicU64::new(0),
        }
    }
}

impl HealthState {
    /// Create a new health state tracker with default config.
    pub fn new() -> Self {
        Self::with_config(HealthConfig::default())
    }

    /// Create a new health state tracker with custom config.
    pub fn with_config(config: HealthConfig) -> Self {
        Self {
            servers: DashMap::new(),
            config,
        }
    }

    /// Register a server for health tracking.
    pub fn register_server(&self, server: SocketAddr) {
        self.servers.entry(server).or_default();
    }

    /// Check if a server is healthy.
    pub fn is_healthy(&self, server: SocketAddr) -> bool {
        self.servers
            .get(&server)
            .map(|s| s.healthy.load(Ordering::Acquire))
            .unwrap_or(true) // Unknown servers are assumed healthy
    }

    /// Check if a server is in cooldown period.
    pub fn is_in_cooldown(&self, server: SocketAddr) -> bool {
        if let Some(health) = self.servers.get(&server) {
            let unhealthy_since = health.unhealthy_since.load(Ordering::Acquire);
            if unhealthy_since == 0 {
                return false; // Server is healthy
            }

            let now = current_timestamp();
            let elapsed = Duration::from_secs(now.saturating_sub(unhealthy_since));
            elapsed < self.config.cooldown
        } else {
            false
        }
    }

    /// Record a successful health check or request.
    pub fn record_success(&self, server: SocketAddr) {
        let entry = self.servers.entry(server).or_default();

        // Reset failures, increment successes
        entry.consecutive_failures.store(0, Ordering::Release);
        let successes = entry.consecutive_successes.fetch_add(1, Ordering::AcqRel) + 1;
        entry.last_check.store(current_timestamp(), Ordering::Release);

        // Check if server should become healthy
        if !entry.healthy.load(Ordering::Acquire) && successes >= self.config.healthy_threshold {
            entry.healthy.store(true, Ordering::Release);
            entry.unhealthy_since.store(0, Ordering::Release);
            entry.consecutive_successes.store(0, Ordering::Release);
            tracing::info!(server = %server, "server marked healthy after {} successes", successes);
        }
    }

    /// Record a failed health check or request.
    pub fn record_failure(&self, server: SocketAddr) {
        let entry = self.servers.entry(server).or_default();

        // Reset successes, increment failures
        entry.consecutive_successes.store(0, Ordering::Release);
        let failures = entry.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        entry.last_check.store(current_timestamp(), Ordering::Release);

        // Check if server should become unhealthy
        if entry.healthy.load(Ordering::Acquire) && failures >= self.config.unhealthy_threshold {
            entry.healthy.store(false, Ordering::Release);
            entry.unhealthy_since.store(current_timestamp(), Ordering::Release);
            entry.consecutive_failures.store(0, Ordering::Release);
            tracing::warn!(server = %server, "server marked unhealthy after {} failures", failures);
        }
    }

    /// Increment active connection count.
    pub fn increment_connections(&self, server: SocketAddr) {
        if let Some(health) = self.servers.get(&server) {
            health.active_connections.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrement active connection count.
    pub fn decrement_connections(&self, server: SocketAddr) {
        if let Some(health) = self.servers.get(&server) {
            let current = health.active_connections.load(Ordering::Relaxed);
            if current > 0 {
                health.active_connections.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    /// Get active connection count for a server.
    pub fn get_connections(&self, server: SocketAddr) -> u32 {
        self.servers
            .get(&server)
            .map(|s| s.active_connections.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get consecutive failures for a server.
    pub fn get_failures(&self, server: SocketAddr) -> u32 {
        self.servers
            .get(&server)
            .map(|s| s.consecutive_failures.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get all healthy servers from a list.
    pub fn filter_healthy(&self, servers: &[SocketAddr]) -> Vec<SocketAddr> {
        servers
            .iter()
            .filter(|&&s| self.is_healthy(s) && !self.is_in_cooldown(s))
            .copied()
            .collect()
    }

    /// Get health status for all registered servers.
    pub fn get_all_status(&self) -> Vec<(SocketAddr, bool, u32, u32)> {
        self.servers
            .iter()
            .map(|entry| {
                let server = *entry.key();
                let health = entry.value();
                (
                    server,
                    health.healthy.load(Ordering::Relaxed),
                    health.active_connections.load(Ordering::Relaxed),
                    health.consecutive_failures.load(Ordering::Relaxed),
                )
            })
            .collect()
    }

    /// Mark a server as explicitly unhealthy (e.g., from passive check).
    pub fn mark_unhealthy(&self, server: SocketAddr) {
        if let Some(health) = self.servers.get(&server) {
            if health.healthy.load(Ordering::Acquire) {
                health.healthy.store(false, Ordering::Release);
                health.unhealthy_since.store(current_timestamp(), Ordering::Release);
                health.consecutive_failures.store(0, Ordering::Release);
                health.consecutive_successes.store(0, Ordering::Release);
                tracing::warn!(server = %server, "server explicitly marked unhealthy");
            }
        }
    }

    /// Reset health state for a server (make it healthy).
    pub fn reset_server(&self, server: SocketAddr) {
        if let Some(health) = self.servers.get(&server) {
            health.healthy.store(true, Ordering::Release);
            health.unhealthy_since.store(0, Ordering::Release);
            health.consecutive_failures.store(0, Ordering::Release);
            health.consecutive_successes.store(0, Ordering::Release);
        }
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the current Unix timestamp in seconds.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_starts_healthy() {
        let state = HealthState::new();
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        state.register_server(server);

        assert!(state.is_healthy(server));
    }

    #[test]
    fn test_failures_mark_unhealthy() {
        let config = HealthConfig {
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(1),
        };
        let state = HealthState::with_config(config);
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        state.register_server(server);

        // First two failures should not mark unhealthy
        state.record_failure(server);
        assert!(state.is_healthy(server));

        state.record_failure(server);
        assert!(state.is_healthy(server));

        // Third failure should mark unhealthy
        state.record_failure(server);
        assert!(!state.is_healthy(server));
    }

    #[test]
    fn test_successes_mark_healthy() {
        let config = HealthConfig {
            unhealthy_threshold: 1,
            healthy_threshold: 2,
            cooldown: Duration::from_millis(1),
        };
        let state = HealthState::with_config(config);
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        state.register_server(server);

        // Make server unhealthy
        state.record_failure(server);
        assert!(!state.is_healthy(server));

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(5));

        // First success
        state.record_success(server);
        assert!(!state.is_healthy(server));

        // Second success should mark healthy
        state.record_success(server);
        assert!(state.is_healthy(server));
    }

    #[test]
    fn test_success_resets_failures() {
        let config = HealthConfig {
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(1),
        };
        let state = HealthState::with_config(config);
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        state.register_server(server);

        // Two failures
        state.record_failure(server);
        state.record_failure(server);
        assert!(state.is_healthy(server));

        // Success resets counter
        state.record_success(server);

        // Two more failures should not mark unhealthy (counter reset)
        state.record_failure(server);
        state.record_failure(server);
        assert!(state.is_healthy(server));

        // Third failure marks unhealthy
        state.record_failure(server);
        assert!(!state.is_healthy(server));
    }

    #[test]
    fn test_connection_tracking() {
        let state = HealthState::new();
        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        state.register_server(server);

        assert_eq!(state.get_connections(server), 0);

        state.increment_connections(server);
        assert_eq!(state.get_connections(server), 1);

        state.increment_connections(server);
        assert_eq!(state.get_connections(server), 2);

        state.decrement_connections(server);
        assert_eq!(state.get_connections(server), 1);
    }

    #[test]
    fn test_filter_healthy() {
        let config = HealthConfig {
            unhealthy_threshold: 1,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(60),
        };
        let state = HealthState::with_config(config);

        let s1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let s2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        let s3: SocketAddr = "127.0.0.1:8003".parse().unwrap();

        state.register_server(s1);
        state.register_server(s2);
        state.register_server(s3);

        // Make s2 unhealthy
        state.record_failure(s2);

        let healthy = state.filter_healthy(&[s1, s2, s3]);
        assert_eq!(healthy.len(), 2);
        assert!(healthy.contains(&s1));
        assert!(healthy.contains(&s3));
        assert!(!healthy.contains(&s2));
    }
}
