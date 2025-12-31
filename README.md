# rustlb

A high-performance Layer 4/7 load balancer written in Rust.

## Features

- **Protocol Support**: TCP (Layer 4) and HTTP/1.1 (Layer 7) load balancing
- **Load Balancing Algorithms**:
  - Round-robin
  - Weighted round-robin
  - Least connections
  - IP hash (sticky sessions)
- **Health Checking**:
  - Active health checks (TCP connect, HTTP requests)
  - Passive health tracking (request failures)
  - Configurable thresholds and cooldown periods
- **Observability**:
  - Prometheus metrics endpoint
  - Structured JSON logging
  - Request-level tracing with request IDs
- **Operations**:
  - Hot configuration reload (SIGHUP)
  - Graceful shutdown (SIGTERM)
  - Config file watching for automatic reload

## Quick Start

### Installation

```bash
# Build from source
git clone https://github.com/jeffgicharu/load-balancer.git
cd load-balancer
cargo build --release

# Binary is at target/release/rustlb
```

### Basic Usage

1. Create a configuration file `config.yaml`:

```yaml
global:
  log_level: info
  log_format: json

frontends:
  - name: web
    listen: "0.0.0.0:8080"
    protocol: http
    backend: web-servers
    algorithm: round_robin

backends:
  - name: web-servers
    servers:
      - address: "127.0.0.1:9001"
      - address: "127.0.0.1:9002"
    health_check:
      type: http
      path: /health
      expected_status: 200
```

2. Run the load balancer:

```bash
rustlb --config config.yaml
```

3. Test it:

```bash
curl http://localhost:8080/
```

### Command Line Options

```
Usage: rustlb [OPTIONS] --config <FILE>

Options:
  -c, --config <FILE>    Path to the configuration file
  -l, --log-level <LEVEL> Override log level (trace, debug, info, warn, error)
      --validate         Validate configuration and exit
      --no-watch         Disable config file watching
  -h, --help             Print help
  -V, --version          Print version
```

## Configuration

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the complete configuration reference.

### Example Configurations

- [examples/simple.yaml](examples/simple.yaml) - Minimal HTTP load balancer
- [examples/tcp-proxy.yaml](examples/tcp-proxy.yaml) - TCP database proxy
- [examples/weighted.yaml](examples/weighted.yaml) - Weighted load balancing
- [examples/full-featured.yaml](examples/full-featured.yaml) - All features enabled

## Metrics

When enabled (default), metrics are exposed at `http://127.0.0.1:9090/metrics` in Prometheus format.

### Available Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rustlb_requests` | Counter | Total requests by frontend, backend, method, status |
| `rustlb_request_duration_seconds` | Histogram | Request latency distribution |
| `rustlb_active_connections` | Gauge | Current active connections |
| `rustlb_connections` | Counter | Total connections |
| `rustlb_bytes` | Counter | Bytes transferred (inbound/outbound) |
| `rustlb_backend_health` | Gauge | Backend health status (1=healthy, 0=unhealthy) |
| `rustlb_health_checks` | Counter | Health check results |

## Signals

| Signal | Action |
|--------|--------|
| `SIGTERM` / `SIGINT` | Graceful shutdown (waits up to 30s for connections to drain) |
| `SIGHUP` | Reload configuration |

## Deployment

### Docker

```bash
docker build -t rustlb .
docker run -v $(pwd)/config.yaml:/etc/rustlb/config.yaml -p 8080:8080 rustlb
```

### Systemd

```bash
# Copy binary
sudo cp target/release/rustlb /usr/local/bin/

# Copy service file
sudo cp rustlb.service /etc/systemd/system/

# Create config directory
sudo mkdir -p /etc/rustlb
sudo cp config.yaml /etc/rustlb/

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable rustlb
sudo systemctl start rustlb
```

## Development

### Building

```bash
cargo build          # Debug build
cargo build --release # Release build
```

### Testing

```bash
cargo test           # Run all tests
cargo test --release # Run tests in release mode
```

### Linting

```bash
cargo clippy         # Run linter
cargo fmt --check    # Check formatting
```

### Benchmarking

```bash
cargo bench          # Run benchmarks
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        rustlb                                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │  Frontend   │    │   Backend   │    │   Health    │     │
│  │  Listener   │───▶│   Router    │◀───│   Checker   │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                  │                  │             │
│         ▼                  ▼                  ▼             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │  TCP/HTTP   │    │  Algorithm  │    │   Health    │     │
│  │   Proxy     │    │  Selection  │    │   State     │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│                                                              │
│  ┌─────────────┐    ┌─────────────┐                        │
│  │   Metrics   │    │   Config    │                        │
│  │   Server    │    │   Watcher   │                        │
│  └─────────────┘    └─────────────┘                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
