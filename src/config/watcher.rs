//! Configuration file watcher for hot reload.
//!
//! Watches the configuration file for changes and triggers reload.

use crate::config::{load_config, validate_config, Config};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Callback type for config reload.
pub type ReloadCallback = Box<dyn Fn(Config) + Send + Sync>;

/// Configuration file watcher.
pub struct ConfigWatcher {
    /// Path to the config file.
    config_path: PathBuf,
    /// Callback to invoke when config is reloaded.
    reload_callback: ReloadCallback,
}

impl ConfigWatcher {
    /// Create a new config watcher.
    pub fn new(config_path: PathBuf, reload_callback: ReloadCallback) -> Self {
        Self {
            config_path,
            reload_callback,
        }
    }

    /// Get the path being watched.
    pub fn path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Run the config watcher.
    ///
    /// This watches for:
    /// - File changes to the config file
    /// - SIGHUP signal for manual reload
    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) {
        info!(path = %self.config_path.display(), "config watcher starting");

        // Create file watcher channel
        let (tx, rx) = mpsc::channel();

        // Create file watcher
        let watcher_result: Result<RecommendedWatcher, _> = Watcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        );

        let mut watcher = match watcher_result {
            Ok(w) => w,
            Err(e) => {
                error!(error = %e, "failed to create file watcher");
                // Just wait for shutdown without watching
                let _ = shutdown.recv().await;
                return;
            }
        };

        // Watch the config file's parent directory
        if let Some(parent) = self.config_path.parent() {
            if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                error!(error = %e, "failed to watch config directory");
                let _ = shutdown.recv().await;
                return;
            }
        }

        // Setup SIGHUP handler (Unix only)
        #[cfg(unix)]
        let mut sighup = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(error = %e, "failed to setup SIGHUP handler");
                None
            }
        };

        info!("config watcher ready, watching for changes");

        loop {
            tokio::select! {
                // Check for file changes (polling)
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Process any pending file events
                    while let Ok(event) = rx.try_recv() {
                        if self.should_reload(&event) {
                            self.try_reload();
                        }
                    }
                }

                // Handle SIGHUP (Unix only)
                _ = async {
                    #[cfg(unix)]
                    {
                        if let Some(ref mut sig) = sighup {
                            sig.recv().await
                        } else {
                            std::future::pending::<Option<()>>().await
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        std::future::pending::<Option<()>>().await
                    }
                } => {
                    info!("received SIGHUP, reloading configuration");
                    self.try_reload();
                }

                // Handle shutdown
                _ = shutdown.recv() => {
                    info!("config watcher shutting down");
                    break;
                }
            }
        }
    }

    /// Check if this event should trigger a reload.
    fn should_reload(&self, event: &Event) -> bool {
        // Check if the event is for our config file
        let is_our_file = event.paths.iter().any(|p| {
            p.file_name() == self.config_path.file_name()
        });

        let is_modify_or_create = matches!(
            event.kind,
            notify::EventKind::Modify(_) | notify::EventKind::Create(_)
        );

        is_our_file && is_modify_or_create
    }

    /// Try to reload the configuration.
    fn try_reload(&self) {
        info!(path = %self.config_path.display(), "attempting config reload");

        // Load the new config
        let new_config = match load_config(&self.config_path) {
            Ok(config) => config,
            Err(e) => {
                error!(error = %e, "failed to load new config, keeping current");
                return;
            }
        };

        // Validate the new config
        if let Err(e) = validate_config(&new_config) {
            error!(error = %e, "new config validation failed, keeping current");
            return;
        }

        // Apply the new config via callback
        info!(
            frontends = new_config.frontends.len(),
            backends = new_config.backends.len(),
            "configuration reloaded successfully"
        );
        (self.reload_callback)(new_config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_reload_modify() {
        let callback: ReloadCallback = Box::new(|_| {});
        let watcher = ConfigWatcher::new(PathBuf::from("/test/config.yaml"), callback);

        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("/test/config.yaml")],
            attrs: Default::default(),
        };

        assert!(watcher.should_reload(&event));
    }

    #[test]
    fn test_should_reload_create() {
        let callback: ReloadCallback = Box::new(|_| {});
        let watcher = ConfigWatcher::new(PathBuf::from("/test/config.yaml"), callback);

        let event = Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/test/config.yaml")],
            attrs: Default::default(),
        };

        assert!(watcher.should_reload(&event));
    }

    #[test]
    fn test_should_reload_wrong_file() {
        let callback: ReloadCallback = Box::new(|_| {});
        let watcher = ConfigWatcher::new(PathBuf::from("/test/config.yaml"), callback);

        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("/test/other.yaml")],
            attrs: Default::default(),
        };

        assert!(!watcher.should_reload(&event));
    }

    #[test]
    fn test_should_reload_delete_ignored() {
        let callback: ReloadCallback = Box::new(|_| {});
        let watcher = ConfigWatcher::new(PathBuf::from("/test/config.yaml"), callback);

        let event = Event {
            kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("/test/config.yaml")],
            attrs: Default::default(),
        };

        assert!(!watcher.should_reload(&event));
    }
}
