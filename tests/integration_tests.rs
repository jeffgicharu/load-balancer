//! Integration tests for rustlb.
//!
//! These tests verify the full load balancer functionality.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

/// Helper to create a simple TCP echo server.
fn start_echo_server(addr: &str) -> (SocketAddr, Arc<AtomicU32>) {
    let listener = TcpListener::bind(addr).expect("failed to bind");
    let addr = listener.local_addr().unwrap();
    let request_count = Arc::new(AtomicU32::new(0));
    let count = Arc::clone(&request_count);

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            count.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 1024];
            if let Ok(n) = stream.read(&mut buf) {
                let _ = stream.write_all(&buf[..n]);
            }
        }
    });

    (addr, request_count)
}

/// Helper to create a simple HTTP server.
fn start_http_server(addr: &str, response_body: &'static str) -> (SocketAddr, Arc<AtomicU32>) {
    let listener = TcpListener::bind(addr).expect("failed to bind");
    let addr = listener.local_addr().unwrap();
    let request_count = Arc::new(AtomicU32::new(0));
    let count = Arc::clone(&request_count);

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            count.fetch_add(1, Ordering::SeqCst);

            // Read request (simple, just consume it)
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);

            // Send response
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (addr, request_count)
}

#[test]
fn test_tcp_echo_server_helper() {
    let (addr, count) = start_echo_server("127.0.0.1:0");

    // Connect and send data
    let mut client = TcpStream::connect(addr).expect("failed to connect");
    client.write_all(b"hello").expect("failed to write");

    let mut response = [0u8; 5];
    client.read_exact(&mut response).expect("failed to read");

    assert_eq!(&response, b"hello");
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_http_server_helper() {
    let (addr, count) = start_http_server("127.0.0.1:0", "OK");

    // Connect and send HTTP request
    let mut client = TcpStream::connect(addr).expect("failed to connect");
    client.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("failed to write");

    let mut response = String::new();
    client.read_to_string(&mut response).expect("failed to read");

    assert!(response.contains("200 OK"));
    assert!(response.contains("OK"));
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_config_parsing() {
    use rustlb::config::load_config;
    use tempfile::NamedTempFile;
    use std::io::Write as IoWrite;

    let config_content = r#"
global:
  log_level: info

frontends:
  - name: test
    listen: "127.0.0.1:0"
    protocol: http
    backend: test-backend
    algorithm: round_robin

backends:
  - name: test-backend
    servers:
      - address: "127.0.0.1:9000"
"#;

    let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
    temp_file.write_all(config_content.as_bytes()).expect("failed to write config");

    let config = load_config(temp_file.path()).expect("failed to load config");

    assert_eq!(config.frontends.len(), 1);
    assert_eq!(config.frontends[0].name, "test");
    assert_eq!(config.backends.len(), 1);
    assert_eq!(config.backends[0].servers.len(), 1);
}

#[test]
fn test_config_validation_missing_backend() {
    use rustlb::config::load_config;
    use tempfile::NamedTempFile;
    use std::io::Write as IoWrite;

    let config_content = r#"
frontends:
  - name: test
    listen: "127.0.0.1:0"
    protocol: http
    backend: nonexistent-backend
    algorithm: round_robin

backends: []
"#;

    let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
    temp_file.write_all(config_content.as_bytes()).expect("failed to write config");

    // Config load validates, so this should fail
    let config = load_config(temp_file.path());
    // The load should fail because validation catches the missing backend
    assert!(config.is_err());
}

#[test]
fn test_backend_router_round_robin() {
    use rustlb::backend::BackendRouter;
    use rustlb::config::{Algorithm, BackendConfig, FrontendConfig, Protocol, ServerConfig};

    let backends = vec![BackendConfig {
        name: "test".to_string(),
        servers: vec![
            ServerConfig {
                address: "127.0.0.1:9001".parse().unwrap(),
                weight: 1,
            },
            ServerConfig {
                address: "127.0.0.1:9002".parse().unwrap(),
                weight: 1,
            },
        ],
        health_check: None,
    }];

    let frontends = vec![FrontendConfig {
        name: "test-frontend".to_string(),
        listen: "127.0.0.1:0".parse().unwrap(),
        protocol: Protocol::Http,
        backend: "test".to_string(),
        algorithm: Algorithm::RoundRobin,
        http: None,
        tcp: None,
    }];

    let router = BackendRouter::new(&backends, &frontends);

    // Round-robin should alternate between servers
    let addr1 = router.select("test", None).unwrap();
    let addr2 = router.select("test", None).unwrap();
    let addr3 = router.select("test", None).unwrap();

    // Should cycle through the servers
    assert_ne!(addr1, addr2);
    assert_eq!(addr1, addr3);
}

#[test]
fn test_backend_router_weighted() {
    use rustlb::backend::BackendRouter;
    use rustlb::config::{Algorithm, BackendConfig, FrontendConfig, Protocol, ServerConfig};

    let backends = vec![BackendConfig {
        name: "test".to_string(),
        servers: vec![
            ServerConfig {
                address: "127.0.0.1:9001".parse().unwrap(),
                weight: 3,
            },
            ServerConfig {
                address: "127.0.0.1:9002".parse().unwrap(),
                weight: 1,
            },
        ],
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

    // Count selections over many iterations
    let mut count_9001 = 0;
    let mut count_9002 = 0;

    for _ in 0..100 {
        let addr = router.select("test", None).unwrap();
        if addr.port() == 9001 {
            count_9001 += 1;
        } else {
            count_9002 += 1;
        }
    }

    // Weight ratio is 3:1, so 9001 should get ~75% of requests
    assert!(count_9001 > count_9002 * 2, "weighted distribution incorrect: {} vs {}", count_9001, count_9002);
}

#[test]
fn test_backend_router_ip_hash() {
    use rustlb::backend::BackendRouter;
    use rustlb::config::{Algorithm, BackendConfig, FrontendConfig, Protocol, ServerConfig};

    let backends = vec![BackendConfig {
        name: "test".to_string(),
        servers: vec![
            ServerConfig {
                address: "127.0.0.1:9001".parse().unwrap(),
                weight: 1,
            },
            ServerConfig {
                address: "127.0.0.1:9002".parse().unwrap(),
                weight: 1,
            },
        ],
        health_check: None,
    }];

    let frontends = vec![FrontendConfig {
        name: "test-frontend".to_string(),
        listen: "127.0.0.1:0".parse().unwrap(),
        protocol: Protocol::Http,
        backend: "test".to_string(),
        algorithm: Algorithm::IpHash,
        http: None,
        tcp: None,
    }];

    let router = BackendRouter::new(&backends, &frontends);

    let client_addr: SocketAddr = "192.168.1.100:12345".parse().unwrap();

    // Same client should always get the same server
    let addr1 = router.select("test", Some(client_addr)).unwrap();
    let addr2 = router.select("test", Some(client_addr)).unwrap();
    let addr3 = router.select("test", Some(client_addr)).unwrap();

    assert_eq!(addr1, addr2);
    assert_eq!(addr2, addr3);
}

#[test]
fn test_health_state() {
    use rustlb::health::{HealthConfig, HealthState};
    use std::time::Duration;

    let config = HealthConfig {
        unhealthy_threshold: 2,
        healthy_threshold: 2,
        cooldown: Duration::from_millis(100),
    };

    let state = HealthState::with_config(config);
    let server: SocketAddr = "127.0.0.1:8000".parse().unwrap();

    // New server is healthy
    state.register_server(server);
    assert!(state.is_healthy(server));

    // One failure - still healthy
    state.record_failure(server);
    assert!(state.is_healthy(server));

    // Two failures - now unhealthy
    state.record_failure(server);
    assert!(!state.is_healthy(server));

    // Wait for cooldown
    thread::sleep(Duration::from_millis(150));

    // One success - still unhealthy
    state.record_success(server);
    assert!(!state.is_healthy(server));

    // Two successes - now healthy again
    state.record_success(server);
    assert!(state.is_healthy(server));
}

#[test]
fn test_metrics_collector() {
    use rustlb::metrics::MetricsCollector;
    use std::time::Duration;

    let collector = MetricsCollector::new();

    // Record various metrics
    collector.record_request("web", "api", "GET", 200, Duration::from_millis(10));
    collector.record_request("web", "api", "POST", 201, Duration::from_millis(20));
    collector.record_request("web", "api", "GET", 500, Duration::from_millis(100));

    collector.connection_opened("web", "api");
    collector.connection_opened("web", "api");
    collector.connection_closed("web", "api");

    collector.set_backend_health("api", "127.0.0.1:8000".parse().unwrap(), true);
    collector.set_backend_health("api", "127.0.0.1:8001".parse().unwrap(), false);

    // Encode metrics to verify they're recorded
    let mut buffer = String::new();
    prometheus_client::encoding::text::encode(&mut buffer, collector.registry()).unwrap();

    assert!(buffer.contains("rustlb_requests"));
    assert!(buffer.contains("rustlb_active_connections"));
    assert!(buffer.contains("rustlb_backend_health"));
}

#[test]
fn test_request_id_generation() {
    use rustlb::util::{generate_request_id, generate_short_request_id, RequestId};
    use std::collections::HashSet;

    // UUID generation
    let id1 = generate_request_id();
    let id2 = generate_request_id();
    assert_ne!(id1, id2);
    assert_eq!(id1.len(), 36); // UUID format

    // Short ID generation
    let short1 = generate_short_request_id();
    let short2 = generate_short_request_id();
    assert_ne!(short1, short2);
    assert!(short1.starts_with("req-"));

    // Uniqueness over many iterations
    let mut ids = HashSet::new();
    for _ in 0..1000 {
        let id = RequestId::short();
        assert!(ids.insert(id.as_str().to_string()));
    }
}
