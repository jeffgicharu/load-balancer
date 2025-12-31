//! Benchmarks for rustlb components.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rustlb::backend::BackendRouter;
use rustlb::config::{Algorithm, BackendConfig, FrontendConfig, Protocol, ServerConfig};
use rustlb::health::{HealthConfig, HealthState};
use rustlb::metrics::MetricsCollector;
use rustlb::util::{generate_request_id, generate_short_request_id};
use std::sync::Arc;
use std::time::Duration;

fn create_router(algorithm: Algorithm, num_servers: usize) -> BackendRouter {
    let servers: Vec<ServerConfig> = (0..num_servers)
        .map(|i| ServerConfig {
            address: format!("127.0.0.1:{}", 9000 + i).parse().unwrap(),
            weight: 1,
        })
        .collect();

    let backends = vec![BackendConfig {
        name: "test".to_string(),
        servers,
        health_check: None,
    }];

    let frontends = vec![FrontendConfig {
        name: "test-frontend".to_string(),
        listen: "127.0.0.1:0".parse().unwrap(),
        protocol: Protocol::Http,
        backend: "test".to_string(),
        algorithm,
        http: None,
        tcp: None,
    }];

    BackendRouter::new(&backends, &frontends)
}

fn benchmark_round_robin(c: &mut Criterion) {
    let router = create_router(Algorithm::RoundRobin, 10);

    c.bench_function("round_robin_select", |b| {
        b.iter(|| {
            black_box(router.select("test", None));
        })
    });
}

fn benchmark_weighted(c: &mut Criterion) {
    let servers: Vec<ServerConfig> = (0..10)
        .map(|i| ServerConfig {
            address: format!("127.0.0.1:{}", 9000 + i).parse().unwrap(),
            weight: (i + 1) as u32,
        })
        .collect();

    let backends = vec![BackendConfig {
        name: "test".to_string(),
        servers,
        health_check: None,
    }];

    let frontends = vec![FrontendConfig {
        name: "test-frontend".to_string(),
        listen: "127.0.0.1:0".parse().unwrap(),
        protocol: Protocol::Http,
        backend: "test".to_string(),
        algorithm: Algorithm::Weighted,
        http: None,
        tcp: None,
    }];

    let router = BackendRouter::new(&backends, &frontends);

    c.bench_function("weighted_select", |b| {
        b.iter(|| {
            black_box(router.select("test", None));
        })
    });
}

fn benchmark_least_connections(c: &mut Criterion) {
    let router = create_router(Algorithm::LeastConnections, 10);

    // Simulate some connections
    for i in 0..5 {
        let addr = format!("127.0.0.1:{}", 9000 + i).parse().unwrap();
        for _ in 0..i {
            router.on_connect("test", addr);
        }
    }

    c.bench_function("least_connections_select", |b| {
        b.iter(|| {
            black_box(router.select("test", None));
        })
    });
}

fn benchmark_ip_hash(c: &mut Criterion) {
    let router = create_router(Algorithm::IpHash, 10);
    let client_addr = "192.168.1.100:12345".parse().unwrap();

    c.bench_function("ip_hash_select", |b| {
        b.iter(|| {
            black_box(router.select("test", Some(client_addr)));
        })
    });
}

fn benchmark_health_state(c: &mut Criterion) {
    let config = HealthConfig {
        unhealthy_threshold: 3,
        healthy_threshold: 2,
        cooldown: Duration::from_secs(30),
    };
    let state = Arc::new(HealthState::with_config(config));

    // Register servers
    for i in 0..100 {
        let addr = format!("127.0.0.1:{}", 9000 + i).parse().unwrap();
        state.register_server(addr);
    }

    let server = "127.0.0.1:9050".parse().unwrap();

    let mut group = c.benchmark_group("health_state");

    group.bench_function("is_healthy", |b| {
        b.iter(|| {
            black_box(state.is_healthy(server));
        })
    });

    group.bench_function("record_success", |b| {
        b.iter(|| {
            state.record_success(server);
        })
    });

    group.bench_function("record_failure", |b| {
        b.iter(|| {
            state.record_failure(server);
        })
    });

    group.finish();
}

fn benchmark_metrics(c: &mut Criterion) {
    let collector = MetricsCollector::new();

    let mut group = c.benchmark_group("metrics");
    group.throughput(Throughput::Elements(1));

    group.bench_function("record_request", |b| {
        b.iter(|| {
            collector.record_request(
                black_box("web"),
                black_box("api"),
                black_box("GET"),
                black_box(200),
                black_box(Duration::from_millis(10)),
            );
        })
    });

    group.bench_function("connection_opened", |b| {
        b.iter(|| {
            collector.connection_opened(black_box("web"), black_box("api"));
        })
    });

    group.bench_function("connection_closed", |b| {
        b.iter(|| {
            collector.connection_closed(black_box("web"), black_box("api"));
        })
    });

    group.finish();
}

fn benchmark_request_id(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_id");

    group.bench_function("uuid", |b| {
        b.iter(|| {
            black_box(generate_request_id());
        })
    });

    group.bench_function("short", |b| {
        b.iter(|| {
            black_box(generate_short_request_id());
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_round_robin,
    benchmark_weighted,
    benchmark_least_connections,
    benchmark_ip_hash,
    benchmark_health_state,
    benchmark_metrics,
    benchmark_request_id,
);

criterion_main!(benches);
