# Technical Architecture

## 1. System Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              RUSTLB                                      │
│                                                                          │
│  ┌──────────────┐    ┌──────────────────────────────────────────────┐  │
│  │   Config     │    │              Core Engine                      │  │
│  │   Loader     │───▶│                                               │  │
│  │              │    │  ┌─────────┐  ┌─────────┐  ┌─────────┐       │  │
│  │  - YAML      │    │  │Frontend │  │Frontend │  │Frontend │       │  │
│  │  - Validate  │    │  │   :80   │  │  :8080  │  │  :5432  │       │  │
│  │  - Watch     │    │  └────┬────┘  └────┬────┘  └────┬────┘       │  │
│  └──────────────┘    │       │            │            │             │  │
│                      │       └────────────┼────────────┘             │  │
│  ┌──────────────┐    │                    │                          │  │
│  │   Health     │    │                    ▼                          │  │
│  │   Checker    │    │         ┌──────────────────┐                  │  │
│  │              │◀───│────────▶│  Backend Router  │                  │  │
│  │  - Active    │    │         │                  │                  │  │
│  │  - Passive   │    │         │  - Algorithm     │                  │  │
│  │  - State     │    │         │  - Selection     │                  │  │
│  └──────────────┘    │         │  - Health-aware  │                  │  │
│                      │         └────────┬─────────┘                  │  │
│  ┌──────────────┐    │                  │                            │  │
│  │   Metrics    │    │    ┌─────────────┼─────────────┐              │  │
│  │   Collector  │◀───│    │             │             │              │  │
│  │              │    │    ▼             ▼             ▼              │  │
│  │  - Counters  │    │ ┌──────┐    ┌──────┐    ┌──────┐             │  │
│  │  - Histograms│    │ │Server│    │Server│    │Server│             │  │
│  │  - Gauges    │    │ │  A   │    │  B   │    │  C   │             │  │
│  └──────────────┘    │ └──────┘    └──────┘    └──────┘             │  │
│                      └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility |
|-----------|---------------|
| **Config Loader** | Parse YAML, validate, watch for changes, notify on reload |
| **Frontend** | Accept connections, handle protocol (TCP/HTTP), delegate to router |
| **Backend Router** | Apply algorithm, select healthy server, track connections |
| **Health Checker** | Active probes, track health state, manage cooldowns |
| **Metrics Collector** | Aggregate stats, expose Prometheus endpoint |
| **Connection Handler** | Bidirectional data transfer between client and backend |

---

## 2. Concurrency Model

### Async Architecture (Tokio)

```
Main Thread
    │
    ├── Spawn: Config Watcher Task
    │       └── Watches file, sends reload signal
    │
    ├── Spawn: Health Checker Task
    │       └── Periodic probes, updates shared state
    │
    ├── Spawn: Metrics Server Task
    │       └── HTTP server on :9090
    │
    └── For each Frontend:
            │
            └── Spawn: Listener Task
                    │
                    └── For each Connection:
                            │
                            └── Spawn: Connection Handler Task
                                    ├── Read from client
                                    ├── Write to backend
                                    ├── Read from backend
                                    └── Write to client
```

### Key Principles

1. **One task per connection** - Each client connection gets its own task
2. **Shared state via Arc** - Health state, config, metrics shared across tasks
3. **Lock-free where possible** - Use atomic operations for counters
4. **Channels for coordination** - Reload signals, shutdown notification

### Shared State

```rust
// Shared across all tasks
struct SharedState {
    // Current configuration (swapped atomically on reload)
    config: ArcSwap<Config>,

    // Backend health status (updated by health checker)
    health: Arc<HealthState>,

    // Metrics (atomic counters, lock-free)
    metrics: Arc<Metrics>,

    // Shutdown signal
    shutdown: broadcast::Sender<()>,
}
```

---

## 3. Data Structures

### Configuration

```rust
struct Config {
    global: GlobalConfig,
    health_check_defaults: HealthCheckDefaults,
    frontends: Vec<FrontendConfig>,
    backends: Vec<BackendConfig>,
}

struct FrontendConfig {
    name: String,
    listen: SocketAddr,
    protocol: Protocol,  // Tcp or Http
    backend: String,     // Name of backend pool
    algorithm: Algorithm,
    http: Option<HttpConfig>,
    tcp: Option<TcpConfig>,
}

struct BackendConfig {
    name: String,
    servers: Vec<ServerConfig>,
    health_check: HealthCheckConfig,
}

struct ServerConfig {
    address: SocketAddr,
    weight: u32,  // Default: 1
}

enum Protocol { Tcp, Http }
enum Algorithm { RoundRobin, Weighted, LeastConnections, IpHash }
```

### Health State

```rust
struct HealthState {
    // Per-backend-server health
    servers: DashMap<SocketAddr, ServerHealth>,
}

struct ServerHealth {
    // Is this server currently healthy?
    healthy: AtomicBool,

    // Consecutive failures (for passive checks)
    consecutive_failures: AtomicU32,

    // Active connections (for least-connections)
    active_connections: AtomicU32,

    // When server became unhealthy (for cooldown)
    unhealthy_since: AtomicU64,  // Unix timestamp, 0 if healthy
}
```

### Metrics

```rust
struct Metrics {
    // Total requests by frontend, backend, status
    requests_total: Family<RequestLabels, Counter>,

    // Request duration histogram
    request_duration: Family<RequestLabels, Histogram>,

    // Active connections gauge
    active_connections: Family<FrontendLabels, Gauge>,

    // Backend health (1 = healthy, 0 = unhealthy)
    backend_health: Family<BackendLabels, Gauge>,

    // Bytes transferred
    bytes_sent: Family<BackendLabels, Counter>,
    bytes_received: Family<BackendLabels, Counter>,
}
```

---

## 4. Module Organization

```
src/
├── main.rs              # Entry point, CLI args, startup
├── lib.rs               # Public API (if used as library)
│
├── config/
│   ├── mod.rs           # Config module exports
│   ├── loader.rs        # YAML parsing, validation
│   ├── types.rs         # Config data structures
│   └── watcher.rs       # File watching, hot reload
│
├── frontend/
│   ├── mod.rs           # Frontend module exports
│   ├── listener.rs      # Accept loop, spawn handlers
│   ├── tcp.rs           # TCP protocol handler
│   └── http.rs          # HTTP protocol handler
│
├── backend/
│   ├── mod.rs           # Backend module exports
│   ├── router.rs        # Backend selection logic
│   ├── pool.rs          # Connection pooling (optional)
│   └── algorithms/
│       ├── mod.rs       # Algorithm trait
│       ├── round_robin.rs
│       ├── weighted.rs
│       ├── least_conn.rs
│       └── ip_hash.rs
│
├── health/
│   ├── mod.rs           # Health module exports
│   ├── checker.rs       # Active health check task
│   ├── state.rs         # Health state management
│   └── passive.rs       # Passive failure tracking
│
├── metrics/
│   ├── mod.rs           # Metrics module exports
│   ├── collector.rs     # Metric definitions
│   └── server.rs        # Prometheus HTTP endpoint
│
├── proxy/
│   ├── mod.rs           # Proxy module exports
│   ├── tcp_proxy.rs     # Bidirectional TCP copy
│   └── http_proxy.rs    # HTTP request/response proxy
│
└── util/
    ├── mod.rs           # Utility exports
    ├── shutdown.rs      # Graceful shutdown handling
    └── logging.rs       # Structured logging setup
```

---

## 5. Key Flows

### Flow 1: Startup

```
1. Parse CLI arguments (config path, log level override)
2. Load and validate configuration file
3. Initialize shared state (health, metrics)
4. Start health checker task
5. Start metrics server task
6. Start config watcher task
7. For each frontend:
   a. Bind to listen address
   b. Start listener task
8. Wait for shutdown signal (SIGTERM, SIGINT)
9. Initiate graceful shutdown
```

### Flow 2: HTTP Request Handling

```
1. Accept TCP connection from client
2. Parse HTTP request (method, path, headers)
3. Determine backend pool from frontend config
4. Select server using algorithm (health-aware)
5. Connect to backend server
6. Add/modify headers (X-Forwarded-For, etc.)
7. Forward request to backend
8. Read response from backend
9. Add response headers
10. Forward response to client
11. If keep-alive: goto 2
12. Close connections, update metrics
```

### Flow 3: TCP Proxying

```
1. Accept TCP connection from client
2. Select backend server using algorithm
3. Connect to backend server
4. Spawn two tasks:
   a. Copy client → backend
   b. Copy backend → client
5. Wait for either direction to close
6. Close both connections
7. Update metrics
```

### Flow 4: Health Check

```
Active (periodic):
1. For each configured backend server:
   a. Attempt connection (TCP) or request (HTTP)
   b. If success: increment healthy counter
   c. If failure: increment failure counter
   d. Update health state based on thresholds
2. Sleep for interval
3. Repeat

Passive (on request failure):
1. On connection/request failure to backend:
   a. Increment failure counter
   b. If exceeds threshold: mark unhealthy
```

### Flow 5: Hot Reload

```
1. Detect change (SIGHUP or file modified)
2. Load new configuration file
3. Validate new configuration
4. If invalid: log error, keep old config
5. If valid:
   a. Atomically swap config
   b. Update health checker with new backends
   c. New connections use new config
   d. Existing connections finish with old config
```

---

## 6. Crate Dependencies

### Core Dependencies

| Crate | Purpose | Why |
|-------|---------|-----|
| `tokio` | Async runtime | Industry standard, best ecosystem |
| `hyper` | HTTP implementation | Fast, correct, widely used |
| `serde` + `serde_yaml` | Config parsing | Ergonomic, well-maintained |
| `tracing` | Structured logging | Modern, async-aware |
| `prometheus-client` | Metrics | Official Rust client |
| `clap` | CLI arguments | Best-in-class CLI parsing |

### Utility Dependencies

| Crate | Purpose |
|-------|---------|
| `arc-swap` | Atomic config swapping |
| `dashmap` | Concurrent hashmap for health state |
| `notify` | File system watching |
| `thiserror` | Error type derivation |
| `anyhow` | Application error handling |
| `bytes` | Efficient byte buffers |

### Estimated Total: ~15-20 direct dependencies

---

## 7. Error Handling Strategy

### Error Categories

| Category | Example | Response |
|----------|---------|----------|
| **Config Error** | Invalid YAML, missing field | Exit on startup, log on reload |
| **Bind Error** | Port already in use | Exit with clear error |
| **Backend Error** | Connection refused | Mark unhealthy, try next |
| **Client Error** | Client disconnected | Clean up, log if unexpected |
| **Internal Error** | Bug in our code | Log, don't crash |

### Error Types

```rust
// Config errors (startup/reload)
#[derive(Debug, thiserror::Error)]
enum ConfigError {
    #[error("failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("failed to parse YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("validation error: {0}")]
    ValidationError(String),
}

// Runtime errors (during operation)
#[derive(Debug, thiserror::Error)]
enum ProxyError {
    #[error("backend connection failed: {0}")]
    BackendConnectError(std::io::Error),

    #[error("client disconnected")]
    ClientDisconnected,

    #[error("no healthy backends available")]
    NoHealthyBackends,
}
```

---

## 8. Testing Strategy

### Unit Tests
- Config parsing and validation
- Algorithm selection logic
- Health state transitions
- Header manipulation

### Integration Tests
- Full request flow with mock backends
- Health check behavior
- Hot reload behavior
- Graceful shutdown

### Load Tests
- wrk/hey for HTTP throughput
- Custom script for connection limits
- Memory usage under load

### Chaos Tests
- Kill backends during traffic
- Slow backends (delayed responses)
- Flaky backends (intermittent failures)

---

*Document Version: 1.0*
*Created: 2025-12-31*
