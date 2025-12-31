# Load Balancer Requirements Document

## 1. Problem Statement

### 1.1 The Core Problem
Modern applications need to handle traffic beyond what a single server can manage while remaining available even when individual servers fail. The fundamental challenges are:

1. **Scalability** - A single server has finite capacity (CPU, memory, network)
2. **Availability** - A single server is a single point of failure
3. **Deployability** - Updating a single server requires downtime

### 1.2 The Solution
A load balancer sits between clients and servers, providing:
- **Single entry point** - Clients connect to one address
- **Traffic distribution** - Requests spread across multiple backends
- **Health awareness** - Unhealthy servers are automatically avoided
- **Transparency** - Clients are unaware of the backend topology

### 1.3 Why Build This?
- Demonstrates deep understanding of networking and distributed systems
- Core infrastructure used by every tech company
- Showcases Rust's strengths: performance, safety, concurrency
- Practical tool that can actually be used in production

---

## 2. Target Users

### Primary: Small Production Deployments
- Teams running 2-20 backend servers
- VPS deployments, small Kubernetes clusters
- Startups and small companies
- Developers who need something simpler than HAProxy/nginx

### Secondary: Learning/Portfolio
- Clear, well-documented code
- Educational value in understanding load balancing concepts

---

## 3. Functional Requirements

### 3.1 Protocol Support

#### Layer 4 (TCP)
- Raw TCP proxying for any protocol
- Use cases: databases, game servers, custom protocols, gRPC
- No inspection of payload content

#### Layer 7 (HTTP/1.1)
- HTTP-aware proxying
- Ability to route based on: Host header, URL path, HTTP method
- Header manipulation (add/remove headers)
- HTTP/1.1 keep-alive support

### 3.2 Load Balancing Algorithms

| Algorithm | Description | Use Case |
|-----------|-------------|----------|
| Round Robin | Rotate through servers sequentially | Equal server capacity |
| Weighted Round Robin | Proportional distribution by weight | Mixed server capacity |
| Least Connections | Send to server with fewest active connections | Variable request duration |
| IP Hash | Same client IP always goes to same server | Sticky sessions, stateful apps |

### 3.3 Health Checking

#### Active Health Checks
- Periodically probe backend servers
- Configurable: interval, timeout, threshold
- TCP check: can we connect?
- HTTP check: does /health return 200?

#### Passive Health Checks
- Monitor real traffic for failures
- Mark server unhealthy after N consecutive failures
- Configurable failure threshold

#### Recovery
- Automatically re-enable servers that become healthy
- Configurable recovery threshold (N successful checks)

### 3.4 High Availability Features

- **Automatic failover**: Skip unhealthy servers
- **Graceful degradation**: Continue with reduced capacity
- **Connection draining**: Finish existing requests before removing server

### 3.5 Zero-Downtime Operations

- **Hot configuration reload**: Apply new config without restart (SIGHUP)
- **Graceful shutdown**: Drain connections before exit
- **Backend hot-add/remove**: Change backends without dropping requests

### 3.6 Configuration

#### Format: YAML (or TOML)
Chosen for:
- Human readable and writable
- Version control friendly
- Industry standard for infrastructure

#### Configuration Scope
- Listen addresses and ports
- Backend server definitions
- Health check parameters
- Algorithm selection per frontend
- Logging and metrics settings

#### Hot Reload
- Watch config file for changes, or
- Respond to SIGHUP signal
- Validate new config before applying
- Rollback on invalid config

---

## 4. Non-Functional Requirements

### 4.1 Performance
- Handle 10,000+ concurrent connections
- Sub-millisecond added latency in normal operation
- Minimal memory footprint (target: <50MB base)
- Efficient under high load (no performance cliffs)

### 4.2 Reliability
- No crashes under any input
- Graceful handling of backend failures
- No connection leaks
- Proper resource cleanup on shutdown

### 4.3 Observability

#### Prometheus Metrics
Expose `/metrics` endpoint with:
- `requests_total` - Total requests (by backend, status)
- `request_duration_seconds` - Latency histogram
- `active_connections` - Current connection count
- `backend_health` - Health status per backend (1=healthy, 0=unhealthy)
- `backend_requests_total` - Requests per backend
- `backend_failures_total` - Failures per backend

#### Structured Logging (JSON)
Every request logged with:
- Timestamp
- Client IP
- Backend selected
- Response status (for HTTP)
- Duration
- Bytes transferred

Log levels: ERROR, WARN, INFO, DEBUG, TRACE

### 4.4 Security
- No TLS termination (v1.0) - assume TLS handled upstream
- Timeouts on all operations (prevent slowloris)
- Connection limits (prevent resource exhaustion)
- No execution of external commands

### 4.5 Operability
- Single static binary (no runtime dependencies)
- Runs as non-root user
- Systemd compatible
- Docker friendly
- Clear error messages

---

## 5. Out of Scope (v1.0)

These features are explicitly NOT included in the initial version:

- TLS/HTTPS termination
- HTTP/2 support
- WebSocket support (may work but not tested)
- UDP load balancing
- Service discovery integration (Consul, etcd)
- Clustering/HA of the load balancer itself
- Rate limiting
- Authentication/authorization
- Request/response modification (beyond headers)
- Caching
- Compression
- Web UI

---

## 6. Success Criteria

The project is successful when:

1. **Functional**: All load balancing algorithms work correctly
2. **Reliable**: Handles backend failures gracefully
3. **Observable**: Metrics and logs provide visibility
4. **Documented**: Clear README, config examples, architecture docs
5. **Tested**: Unit tests, integration tests, and benchmarks
6. **Deployable**: Can run in real production environment

---

## 7. Glossary

| Term | Definition |
|------|------------|
| **Frontend** | The listening address/port that accepts client connections |
| **Backend** | An upstream server that handles requests |
| **Health check** | Mechanism to verify backend availability |
| **Layer 4** | Transport layer (TCP) - no application awareness |
| **Layer 7** | Application layer (HTTP) - understands protocol |
| **Hot reload** | Applying configuration changes without restart |
| **Connection draining** | Allowing existing connections to complete before removal |
| **Sticky session** | Routing same client to same backend consistently |

---

## 8. Open Questions

Questions to resolve during design phase:

1. What config file format? YAML vs TOML
2. How to handle very long-lived connections during hot reload?
3. Should we support multiple frontends in one process?
4. What's the circuit breaker policy? (how long to wait before retrying unhealthy backend)
5. How to handle backends with different response times?

---

*Document Version: 1.0*
*Created: 2025-12-31*
*Status: Draft - Ready for Review*
