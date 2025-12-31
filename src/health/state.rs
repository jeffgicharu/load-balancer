//! Health state management.
//!
//! To be implemented in Phase 3.

use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};

/// Shared health state for all backends.
pub struct HealthState {
    servers: DashMap<SocketAddr, ServerHealth>,
}

/// Health information for a single server.
pub struct ServerHealth {
    /// Is this server currently healthy?
    pub healthy: AtomicBool,

    /// Consecutive failures count.
    pub consecutive_failures: AtomicU32,

    /// Active connection count.
    pub active_connections: AtomicU32,

    /// Unix timestamp when server became unhealthy (0 if healthy).
    pub unhealthy_since: AtomicU64,
}

impl HealthState {
    /// Create a new health state tracker.
    pub fn new() -> Self {
        Self {
            servers: DashMap::new(),
        }
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}
