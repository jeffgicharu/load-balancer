# Build stage
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && \
    rm -rf src target/release/deps/rustlb*

# Copy actual source
COPY src ./src

# Build the application
RUN cargo build --release --locked

# Runtime stage
FROM alpine:3.20

# Install runtime dependencies
RUN apk add --no-cache ca-certificates

# Create non-root user
RUN addgroup -g 1000 rustlb && \
    adduser -u 1000 -G rustlb -s /bin/sh -D rustlb

# Create config directory
RUN mkdir -p /etc/rustlb && \
    chown rustlb:rustlb /etc/rustlb

# Copy binary from builder
COPY --from=builder /app/target/release/rustlb /usr/local/bin/rustlb

# Copy example config
COPY examples/simple.yaml /etc/rustlb/config.yaml

# Switch to non-root user
USER rustlb

# Expose default ports
EXPOSE 8080 9090

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:9090/health || exit 1

# Default command
ENTRYPOINT ["rustlb"]
CMD ["--config", "/etc/rustlb/config.yaml"]
