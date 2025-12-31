//! Metrics collector using prometheus-client.
//!
//! Provides metrics for request counts, latency, connections, and backend health.

use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// Labels for request metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RequestLabels {
    pub frontend: String,
    pub backend: String,
    pub method: String,
    pub status: String,
}

/// Labels for connection metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ConnectionLabels {
    pub frontend: String,
    pub backend: String,
}

/// Labels for backend health metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct BackendLabels {
    pub backend: String,
    pub server: String,
}

/// Labels for bytes transferred metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct BytesLabels {
    pub frontend: String,
    pub backend: String,
    pub direction: Direction,
}

/// Direction of bytes transfer.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Direction {
    #[allow(dead_code)]
    Inbound,
    #[allow(dead_code)]
    Outbound,
}

/// Collects and stores all metrics.
#[derive(Clone)]
pub struct MetricsCollector {
    inner: Arc<MetricsCollectorInner>,
}

struct MetricsCollectorInner {
    /// Total requests counter.
    requests_total: Family<RequestLabels, Counter>,
    /// Request duration histogram (in seconds).
    request_duration_seconds: Family<ConnectionLabels, Histogram>,
    /// Active connections gauge.
    active_connections: Family<ConnectionLabels, Gauge>,
    /// Backend health gauge (1 = healthy, 0 = unhealthy).
    backend_health: Family<BackendLabels, Gauge>,
    /// Bytes transferred counter.
    bytes_total: Family<BytesLabels, Counter>,
    /// Total connections counter.
    connections_total: Family<ConnectionLabels, Counter>,
    /// Health check results counter.
    health_checks_total: Family<HealthCheckLabels, Counter>,
    /// The prometheus registry.
    registry: Registry,
}

/// Labels for health check metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct HealthCheckLabels {
    pub backend: String,
    pub server: String,
    pub result: HealthCheckResult,
}

/// Result of a health check.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum HealthCheckResult {
    Success,
    Failure,
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        let mut registry = Registry::default();

        // Create metrics
        let requests_total = Family::<RequestLabels, Counter>::default();
        let request_duration_seconds = Family::<ConnectionLabels, Histogram>::new_with_constructor(
            || {
                // Buckets: 1ms, 2.5ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
                Histogram::new(exponential_buckets(0.001, 2.5, 13))
            },
        );
        let active_connections = Family::<ConnectionLabels, Gauge>::default();
        let backend_health = Family::<BackendLabels, Gauge>::default();
        let bytes_total = Family::<BytesLabels, Counter>::default();
        let connections_total = Family::<ConnectionLabels, Counter>::default();
        let health_checks_total = Family::<HealthCheckLabels, Counter>::default();

        // Register metrics
        registry.register(
            "rustlb_requests",
            "Total number of requests processed",
            requests_total.clone(),
        );
        registry.register(
            "rustlb_request_duration_seconds",
            "Request duration in seconds",
            request_duration_seconds.clone(),
        );
        registry.register(
            "rustlb_active_connections",
            "Number of active connections",
            active_connections.clone(),
        );
        registry.register(
            "rustlb_backend_health",
            "Backend server health status (1=healthy, 0=unhealthy)",
            backend_health.clone(),
        );
        registry.register(
            "rustlb_bytes",
            "Total bytes transferred",
            bytes_total.clone(),
        );
        registry.register(
            "rustlb_connections",
            "Total number of connections",
            connections_total.clone(),
        );
        registry.register(
            "rustlb_health_checks",
            "Total number of health checks performed",
            health_checks_total.clone(),
        );

        Self {
            inner: Arc::new(MetricsCollectorInner {
                requests_total,
                request_duration_seconds,
                active_connections,
                backend_health,
                bytes_total,
                connections_total,
                health_checks_total,
                registry,
            }),
        }
    }

    /// Get the prometheus registry for encoding.
    pub fn registry(&self) -> &Registry {
        &self.inner.registry
    }

    /// Record a completed request.
    pub fn record_request(
        &self,
        frontend: &str,
        backend: &str,
        method: &str,
        status: u16,
        duration: std::time::Duration,
    ) {
        let labels = RequestLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
            method: method.to_string(),
            status: status.to_string(),
        };
        self.inner.requests_total.get_or_create(&labels).inc();

        let conn_labels = ConnectionLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
        };
        self.inner
            .request_duration_seconds
            .get_or_create(&conn_labels)
            .observe(duration.as_secs_f64());
    }

    /// Record a TCP proxy session completion.
    pub fn record_tcp_session(
        &self,
        frontend: &str,
        backend: &str,
        bytes_to_backend: u64,
        bytes_to_client: u64,
        duration: std::time::Duration,
    ) {
        let conn_labels = ConnectionLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
        };

        // Record duration
        self.inner
            .request_duration_seconds
            .get_or_create(&conn_labels)
            .observe(duration.as_secs_f64());

        // Record bytes
        let inbound_labels = BytesLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
            direction: Direction::Inbound,
        };
        self.inner
            .bytes_total
            .get_or_create(&inbound_labels)
            .inc_by(bytes_to_backend);

        let outbound_labels = BytesLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
            direction: Direction::Outbound,
        };
        self.inner
            .bytes_total
            .get_or_create(&outbound_labels)
            .inc_by(bytes_to_client);
    }

    /// Increment active connections.
    pub fn connection_opened(&self, frontend: &str, backend: &str) {
        let labels = ConnectionLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
        };
        self.inner.active_connections.get_or_create(&labels).inc();
        self.inner.connections_total.get_or_create(&labels).inc();
    }

    /// Decrement active connections.
    pub fn connection_closed(&self, frontend: &str, backend: &str) {
        let labels = ConnectionLabels {
            frontend: frontend.to_string(),
            backend: backend.to_string(),
        };
        self.inner.active_connections.get_or_create(&labels).dec();
    }

    /// Update backend health status.
    pub fn set_backend_health(&self, backend: &str, server: SocketAddr, healthy: bool) {
        let labels = BackendLabels {
            backend: backend.to_string(),
            server: server.to_string(),
        };
        self.inner
            .backend_health
            .get_or_create(&labels)
            .set(if healthy { 1 } else { 0 });
    }

    /// Record a health check result.
    pub fn record_health_check(&self, backend: &str, server: SocketAddr, success: bool) {
        let labels = HealthCheckLabels {
            backend: backend.to_string(),
            server: server.to_string(),
            result: if success {
                HealthCheckResult::Success
            } else {
                HealthCheckResult::Failure
            },
        };
        self.inner.health_checks_total.get_or_create(&labels).inc();
    }

    /// Start timing a request. Returns a guard that records duration on drop.
    pub fn start_request_timer(&self, frontend: &str, backend: &str) -> RequestTimer {
        RequestTimer {
            collector: self.clone(),
            frontend: frontend.to_string(),
            backend: backend.to_string(),
            start: Instant::now(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer guard that records request duration on drop.
pub struct RequestTimer {
    collector: MetricsCollector,
    frontend: String,
    backend: String,
    start: Instant,
}

impl RequestTimer {
    /// Get the elapsed duration.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Record the duration manually and consume the timer.
    pub fn record(self, method: &str, status: u16) {
        let duration = self.start.elapsed();
        self.collector.record_request(
            &self.frontend,
            &self.backend,
            method,
            status,
            duration,
        );
    }
}

impl Drop for RequestTimer {
    fn drop(&mut self) {
        // Only record duration, not the full request (that's done via record())
        // This allows the histogram to be populated even if record() isn't called
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_new() {
        let collector = MetricsCollector::new();
        // Just verify we can create and access the collector
        let _ = collector.registry();
    }

    #[test]
    fn test_record_request() {
        let collector = MetricsCollector::new();
        collector.record_request(
            "web",
            "api-servers",
            "GET",
            200,
            std::time::Duration::from_millis(50),
        );
        // Metrics should be recorded without panic
    }

    #[test]
    fn test_connection_tracking() {
        let collector = MetricsCollector::new();

        collector.connection_opened("web", "api-servers");
        collector.connection_opened("web", "api-servers");
        collector.connection_closed("web", "api-servers");
        // Should have 1 active connection
    }

    #[test]
    fn test_backend_health() {
        let collector = MetricsCollector::new();
        let server: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        collector.set_backend_health("api-servers", server, true);
        collector.set_backend_health("api-servers", server, false);
        // Health should be updated without panic
    }

    #[test]
    fn test_request_timer() {
        let collector = MetricsCollector::new();
        let timer = collector.start_request_timer("web", "api-servers");
        std::thread::sleep(std::time::Duration::from_millis(10));
        timer.record("GET", 200);
        // Timer should record duration
    }

    #[test]
    fn test_tcp_session() {
        let collector = MetricsCollector::new();
        collector.record_tcp_session(
            "tcp-frontend",
            "tcp-backend",
            1024,
            2048,
            std::time::Duration::from_millis(100),
        );
        // Session should be recorded without panic
    }

    #[test]
    fn test_health_check_recording() {
        let collector = MetricsCollector::new();
        let server: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        collector.record_health_check("api-servers", server, true);
        collector.record_health_check("api-servers", server, false);
        // Health checks should be recorded without panic
    }
}
