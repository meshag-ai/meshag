# Unified Meshag Service Dockerfile
# Builds a single binary that can run any service type based on SERVICE_TYPE env var

# Build stage - shared across all services
FROM rust:1.88-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY services/ ./services/

# Build all dependencies (this layer will be cached)
RUN cargo build --release -p meshag-service

# Remove dummy file
RUN rm -rf services/meshag-service/src/main.rs

# Copy actual source code
COPY . .

# Build the unified service (dependencies already built)
RUN cargo build --release -p meshag-service

# Runtime stage - using distroless for minimal size and security
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy binary from builder
COPY --from=builder /app/target/release/meshag-service /usr/local/bin/meshag-service

# Expose ports (all services use different ports)
EXPOSE 8080 8081 8082 8083 8084

# Health check (will work for any service type)
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/usr/bin/wget", "--quiet", "--tries=1", "--spider", "http://localhost:${PORT:-8080}/health"]

# Run the service (SERVICE_TYPE env var determines which service to run)
CMD ["/usr/local/bin/meshag-service"]
