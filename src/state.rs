//! Shared application state.

use crate::config::Config;
use crate::health::HealthState;
use crate::util::ShutdownSignal;
use arc_swap::ArcSwap;
use std::sync::Arc;

/// Shared state accessible from all tasks.
#[derive(Clone)]
pub struct AppState {
    /// Current configuration (can be swapped atomically for hot reload).
    config: Arc<ArcSwap<Config>>,

    /// Health state for all backends.
    health: Arc<HealthState>,

    /// Shutdown signal.
    shutdown: ShutdownSignal,
}

impl AppState {
    /// Create new application state.
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
            health: Arc::new(HealthState::new()),
            shutdown: ShutdownSignal::new(),
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    /// Swap the configuration atomically (for hot reload).
    pub fn swap_config(&self, new_config: Config) {
        self.config.store(Arc::new(new_config));
    }

    /// Get the health state.
    pub fn health(&self) -> &Arc<HealthState> {
        &self.health
    }

    /// Get the shutdown signal.
    pub fn shutdown(&self) -> &ShutdownSignal {
        &self.shutdown
    }

    /// Trigger shutdown.
    pub fn trigger_shutdown(&self) {
        self.shutdown.shutdown();
    }
}
