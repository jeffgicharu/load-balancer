//! Metrics collection and exposition.

mod collector;
mod server;

pub use collector::MetricsCollector;
pub use server::MetricsServer;
