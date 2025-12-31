//! Configuration file watcher for hot reload.
//!
//! This module will be implemented in Phase 3.

use std::path::PathBuf;

/// Watches a configuration file for changes and triggers reloads.
pub struct ConfigWatcher {
    path: PathBuf,
}

impl ConfigWatcher {
    /// Create a new configuration watcher.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get the path being watched.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}
