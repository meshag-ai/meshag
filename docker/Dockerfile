# Optimized Multi-stage Dockerfile for Meshag Service
# Uses layer caching and minimal runtime image

# Stage 1: Dependencies builder (cached separately)
FROM rust:1.88-bookworm AS dependency-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy only dependency manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/shared/Cargo.toml ./crates/shared/
COPY crates/connectors/Cargo.toml ./crates/connectors/
COPY crates/service-common/Cargo.toml ./crates/service-common/
COPY crates/orchestrator/Cargo.toml ./crates/orchestrator/
COPY crates/services/llm/Cargo.toml ./crates/services/llm/
COPY crates/services/stt/Cargo.toml ./crates/services/stt/
COPY crates/services/tts/Cargo.toml ./crates/services/tts/
COPY crates/services/transport/Cargo.toml ./crates/services/transport/
COPY crates/meshag-python/Cargo.toml ./crates/meshag-python/
COPY services/meshag-service/Cargo.toml ./services/meshag-service/

# Create dummy source files to build dependencies
RUN mkdir -p crates/shared/src \
    crates/connectors/src \
    crates/service-common/src \
    crates/orchestrator/src \
    crates/services/llm/src \
    crates/services/stt/src \
    crates/services/tts/src \
    crates/services/transport/src \
    crates/meshag-python/src \
    services/meshag-service/src && \
    echo "fn main() {}" > services/meshag-service/src/main.rs && \
    find crates -name "Cargo.toml" -execdir sh -c 'echo "// dummy" > src/lib.rs' \;

# Build only dependencies (this layer will be cached)
RUN cargo build --release --bin meshag-service

# Stage 2: Source builder
FROM rust:1.88-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy pre-built dependencies from previous stage
COPY --from=dependency-builder /app/target ./target
COPY --from=dependency-builder /usr/local/cargo /usr/local/cargo

# Copy all source code
COPY . .

# Build the actual binary (dependencies already compiled)
RUN cargo build --release --bin meshag-service

# Strip debug symbols to reduce size
RUN strip /app/target/release/meshag-service

# Stage 3: Minimal runtime image
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the stripped binary
COPY --from=builder /app/target/release/meshag-service /usr/local/bin/meshag-service

# Set default environment
ENV SERVICE_TYPE=gateway
ENV PORT=8080
ENV RUST_LOG=info

# Expose common ports
EXPOSE 8080 8081 8082 8083

# Non-root user (from distroless)
USER nonroot:nonroot

# Run the service
ENTRYPOINT ["/usr/local/bin/meshag-service"]
