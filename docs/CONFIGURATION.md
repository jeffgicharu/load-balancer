# Configuration Reference

rustlb uses YAML configuration files. This document describes all available options.

## Table of Contents

- [Global Settings](#global-settings)
- [Frontends](#frontends)
- [Backends](#backends)
- [Health Checks](#health-checks)
- [Health Check Defaults](#health-check-defaults)

## Global Settings

```yaml
global:
  log_level: info
  log_format: json
  metrics:
    enabled: true
    address: "127.0.0.1:9090"
    path: /metrics
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `log_level` | string | `info` | Log verbosity: `trace`, `debug`, `info`, `warn`, `error` |
| `log_format` | string | `json` | Log format: `json` or `pretty` |
| `metrics.enabled` | bool | `true` | Enable Prometheus metrics endpoint |
| `metrics.address` | string | `127.0.0.1:9090` | Address for metrics server |
| `metrics.path` | string | `/metrics` | Path for metrics endpoint |

## Frontends

Frontends define where rustlb listens for incoming connections.

```yaml
frontends:
  - name: web
    listen: "0.0.0.0:8080"
    protocol: http
    backend: web-servers
    algorithm: round_robin
    http:
      request_headers:
        X-Custom: "value"
      response_headers:
        X-Served-By: "$backend_name"
    tcp:
      connect_timeout: 10s
```

### Frontend Options

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `name` | string | Yes | - | Unique identifier for this frontend |
| `listen` | string | Yes | - | Address and port to listen on (e.g., `0.0.0.0:8080`) |
| `protocol` | string | No | `tcp` | Protocol: `tcp` or `http` |
| `backend` | string | Yes | - | Name of the backend pool to use |
| `algorithm` | string | No | `round_robin` | Load balancing algorithm |

### Algorithms

| Algorithm | Description |
|-----------|-------------|
| `round_robin` | Distribute requests sequentially across servers |
| `weighted` | Distribute requests proportionally based on server weights |
| `least_connections` | Send to server with fewest active connections |
| `ip_hash` | Consistent hashing based on client IP (sticky sessions) |

### HTTP Options

Only applicable when `protocol: http`.

```yaml
http:
  request_headers:
    X-Forwarded-For: "$client_ip"
    X-Real-IP: "$client_ip"
  response_headers:
    X-Served-By: "$backend_name"
```

| Option | Type | Description |
|--------|------|-------------|
| `request_headers` | map | Headers to add to requests sent to backend |
| `response_headers` | map | Headers to add to responses sent to client |

#### Header Variables

These variables can be used in header values:

| Variable | Description |
|----------|-------------|
| `$client_ip` | Client's IP address |
| `$client_port` | Client's port number |
| `$backend_name` | Name of the backend pool |
| `$backend_addr` | Address of the selected backend server |

### TCP Options

Only applicable when `protocol: tcp`.

```yaml
tcp:
  connect_timeout: 10s
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `connect_timeout` | duration | `10s` | Timeout for connecting to backend |

## Backends

Backends define pools of upstream servers.

```yaml
backends:
  - name: web-servers
    servers:
      - address: "10.0.0.1:8000"
        weight: 3
      - address: "10.0.0.2:8000"
        weight: 1
    health_check:
      type: http
      path: /health
      expected_status: 200
```

### Backend Options

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `name` | string | Yes | Unique identifier for this backend pool |
| `servers` | list | Yes | List of upstream servers |
| `health_check` | object | No | Health check configuration |

### Server Options

```yaml
servers:
  - address: "10.0.0.1:8000"
    weight: 1
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `address` | string | Yes | - | Server address and port |
| `weight` | int | No | `1` | Weight for weighted load balancing |

## Health Checks

Health checks verify that backend servers are healthy.

### TCP Health Check

```yaml
health_check:
  type: tcp
  interval: 10s
  timeout: 5s
```

Simply attempts to establish a TCP connection.

### HTTP Health Check

```yaml
health_check:
  type: http
  path: /health
  expected_status: 200
  interval: 10s
  timeout: 5s
```

Sends an HTTP GET request and checks the response status.

### Health Check Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `type` | string | `tcp` | Check type: `tcp` or `http` |
| `path` | string | `/` | HTTP path to check (HTTP only) |
| `expected_status` | int | `200` | Expected HTTP status code (HTTP only) |
| `interval` | duration | `10s` | Time between health checks |
| `timeout` | duration | `5s` | Timeout for health check response |

## Health Check Defaults

Global defaults for health checks that can be overridden per-backend.

```yaml
health_check_defaults:
  interval: 10s
  timeout: 5s
  unhealthy_threshold: 3
  healthy_threshold: 2
  cooldown: 30s
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `interval` | duration | `10s` | Time between health checks |
| `timeout` | duration | `5s` | Timeout for health check response |
| `unhealthy_threshold` | int | `3` | Consecutive failures before marking unhealthy |
| `healthy_threshold` | int | `2` | Consecutive successes before marking healthy |
| `cooldown` | duration | `30s` | Time before retrying an unhealthy server |

## Duration Format

Duration values support human-readable formats:

- `100ms` - 100 milliseconds
- `5s` - 5 seconds
- `1m` - 1 minute
- `1h` - 1 hour
- `1m30s` - 1 minute and 30 seconds

## Example: Complete Configuration

```yaml
global:
  log_level: info
  log_format: json
  metrics:
    enabled: true
    address: "0.0.0.0:9090"
    path: /metrics

health_check_defaults:
  interval: 10s
  timeout: 5s
  unhealthy_threshold: 3
  healthy_threshold: 2
  cooldown: 30s

frontends:
  - name: web
    listen: "0.0.0.0:80"
    protocol: http
    backend: web-servers
    algorithm: least_connections
    http:
      request_headers:
        X-Real-IP: "$client_ip"
      response_headers:
        X-Served-By: "$backend_name"

  - name: api
    listen: "0.0.0.0:8080"
    protocol: http
    backend: api-servers
    algorithm: ip_hash

backends:
  - name: web-servers
    servers:
      - address: "10.0.1.10:8000"
        weight: 3
      - address: "10.0.1.11:8000"
        weight: 2
    health_check:
      type: http
      path: /healthz
      expected_status: 200

  - name: api-servers
    servers:
      - address: "10.0.2.10:3000"
      - address: "10.0.2.11:3000"
    health_check:
      type: http
      path: /api/health
      expected_status: 200
      interval: 5s
```
