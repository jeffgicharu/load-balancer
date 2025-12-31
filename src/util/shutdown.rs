//! Graceful shutdown handling.

use tokio::sync::broadcast;

/// Manages graceful shutdown signals.
#[derive(Clone)]
pub struct ShutdownSignal {
    sender: broadcast::Sender<()>,
}

impl ShutdownSignal {
    /// Create a new shutdown signal manager.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1);
        Self { sender }
    }

    /// Subscribe to shutdown notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }

    /// Trigger shutdown.
    pub fn shutdown(&self) {
        let _ = self.sender.send(());
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}
