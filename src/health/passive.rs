//! Passive health tracking.
//!
//! Tracks request failures during proxying to detect unhealthy backends.

use crate::health::HealthState;
use std::net::SocketAddr;
use std::sync::Arc;

/// Tracks request failures for passive health checking.
///
/// This is called by the proxy layer when requests succeed or fail.
/// It updates the shared health state based on actual traffic.
#[derive(Clone)]
pub struct PassiveHealthTracker {
    /// Shared health state.
    health_state: Arc<HealthState>,
}

impl PassiveHealthTracker {
    /// Create a new passive health tracker.
    pub fn new(health_state: Arc<HealthState>) -> Self {
        Self { health_state }
    }

    /// Record a successful request to a server.
    pub fn record_success(&self, server: SocketAddr) {
        self.health_state.record_success(server);
    }

    /// Record a failed request to a server.
    ///
    /// This is called when:
    /// - Connection to backend fails
    /// - Backend returns an error response (5xx)
    /// - Request times out
    pub fn record_failure(&self, server: SocketAddr) {
        self.health_state.record_failure(server);
    }

    /// Check if a server is healthy.
    pub fn is_healthy(&self, server: SocketAddr) -> bool {
        self.health_state.is_healthy(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::state::HealthConfig;
    use std::time::Duration;

    #[test]
    fn test_passive_tracking() {
        let config = HealthConfig {
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(1),
        };
        let health_state = Arc::new(HealthState::with_config(config));
        let tracker = PassiveHealthTracker::new(Arc::clone(&health_state));

        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        health_state.register_server(server);

        // Server starts healthy
        assert!(tracker.is_healthy(server));

        // After 3 failures, becomes unhealthy
        tracker.record_failure(server);
        tracker.record_failure(server);
        tracker.record_failure(server);
        assert!(!tracker.is_healthy(server));
    }

    #[test]
    fn test_success_resets_failures() {
        let config = HealthConfig {
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            cooldown: Duration::from_secs(1),
        };
        let health_state = Arc::new(HealthState::with_config(config));
        let tracker = PassiveHealthTracker::new(Arc::clone(&health_state));

        let server: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        health_state.register_server(server);

        // Two failures
        tracker.record_failure(server);
        tracker.record_failure(server);
        assert!(tracker.is_healthy(server));

        // Success resets
        tracker.record_success(server);

        // Two more failures don't make unhealthy
        tracker.record_failure(server);
        tracker.record_failure(server);
        assert!(tracker.is_healthy(server));
    }
}
