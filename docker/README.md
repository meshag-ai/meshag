# Docker Setup for Meshag

## 📁 Files

- **`Dockerfile`** - Optimized multi-stage build with dependency caching
- **`Dockerfile.fast`** - Faster development builds (less optimized)
- **`docker-compose.yml`** - Complete production stack
- **`.dockerignore`** - Excludes unnecessary files from build context

## 🚀 Quick Start

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

## 🎯 Dockerfile Optimization Features

### Main Dockerfile (Production)

✅ **3-Stage Build Process**:
1. **Dependency Builder** - Builds and caches all Cargo dependencies separately
2. **Source Builder** - Compiles actual code (dependencies already cached)
3. **Runtime** - Minimal distroless image (~30MB)

✅ **Size Optimizations**:
- Multi-stage build to exclude build tools
- Distroless base image (no shell, minimal attack surface)
- Stripped binary (removes debug symbols)
- Final image: ~50-80MB (vs 1GB+ with full Rust toolchain)

✅ **Build Speed Optimizations**:
- Dependency layer caching (only rebuilds when Cargo.toml changes)
- Source code changes don't invalidate dependency cache
- Docker BuildKit support with layer caching

✅ **Security**:
- Non-root user (distroless nonroot)
- Minimal attack surface (no shell, no package manager)
- Only essential runtime dependencies

### Dockerfile.fast (Development)

For faster iteration during development:
```bash
docker build -f docker/Dockerfile.fast -t meshag-dev .
```

Features:
- Single dependency resolution (faster first build)
- Debian base with debugging tools (curl, etc.)
- Simpler build process
- Use for rapid prototyping

## 📊 Build Comparison

| Metric | Dockerfile (Prod) | Dockerfile.fast (Dev) |
|--------|-------------------|----------------------|
| First Build | ~15-20 min | ~10-15 min |
| Rebuild (code change) | ~2-3 min | ~10-15 min |
| Image Size | ~80MB | ~250MB |
| Security | High (distroless) | Medium (debian-slim) |
| Debug Tools | ❌ | ✅ |

## 🔧 Build Options

### Build with BuildKit (Faster)

```bash
# Enable BuildKit
export DOCKER_BUILDKIT=1

# Build with cache mounting
docker build \
  --cache-from meshag-service:latest \
  -f docker/Dockerfile \
  -t meshag-service:latest \
  ..
```

### Multi-platform Build

```bash
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -f docker/Dockerfile \
  -t meshag-service:latest \
  ..
```

### Development Build with Fast Dockerfile

```bash
docker-compose -f docker-compose.dev.yml up
```

## 🐳 Image Layers Breakdown

**Optimized Dockerfile:**
```
Layer 1: Base Rust builder (~1GB) [cached]
Layer 2: Dependencies (~500MB) [cached unless Cargo.toml changes]
Layer 3: Source code build (~100MB) [rebuilds on code change]
Layer 4: Runtime distroless (~30MB)
Final: Binary (~50MB)
Total: ~80MB runtime image
```

**Fast Dockerfile:**
```
Layer 1: Base Rust builder (~1GB) [cached]
Layer 2: Full build (~200MB)
Layer 3: Runtime debian-slim (~80MB)
Final: Binary (~50MB)
Total: ~250MB runtime image
```

## 🔒 Security Considerations

### Distroless Image Benefits

1. **No shell** - Prevents shell-based attacks
2. **No package manager** - Can't install malicious packages
3. **Minimal CVEs** - Fewer dependencies = fewer vulnerabilities
4. **Non-root user** - Follows principle of least privilege

### Scanning Images

```bash
# Scan for vulnerabilities
docker scan meshag-service:latest

# Or use Trivy
trivy image meshag-service:latest
```

## 📈 Performance Tips

### Layer Caching

The optimized Dockerfile uses a 3-stage build to maximize layer caching:

1. **Dependencies stage** - Cached until Cargo.toml changes
2. **Build stage** - Uses cached dependencies
3. **Runtime stage** - Minimal final image

### BuildKit Cache Mounts

For even faster builds:

```dockerfile
# In Dockerfile, use cache mounts:
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release
```

### Docker Compose Build Cache

```bash
# Build with inline cache
docker-compose build --build-arg BUILDKIT_INLINE_CACHE=1

# Use cached images
docker-compose up --build
```

## 🛠️ Troubleshooting

### Build is slow

1. Enable BuildKit: `export DOCKER_BUILDKIT=1`
2. Use `Dockerfile.fast` for development
3. Ensure Docker has enough resources (CPU/RAM)

### Image is too large

1. Using optimized `Dockerfile`? (not `.fast`)
2. Check if build artifacts are included (use `.dockerignore`)
3. Binary stripped? (`strip` command in Dockerfile)

### Container won't start

1. Check logs: `docker-compose logs [service-name]`
2. Verify environment variables in `.env`
3. Ensure NATS is healthy: `docker-compose ps`

### Permission errors

Distroless runs as non-root user. If you need to write files:
```yaml
volumes:
  - ./data:/data:rw
# Ensure host directory has correct permissions
```

## 🔄 CI/CD Integration

### GitHub Actions Example

```yaml
name: Build and Push Docker Image

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2

      - name: Build and push
        uses: docker/build-push-action@v4
        with:
          context: .
          file: docker/Dockerfile
          push: true
          tags: yourusername/meshag-service:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

## 📚 References

- [Docker Multi-stage Builds](https://docs.docker.com/build/building/multi-stage/)
- [Distroless Images](https://github.com/GoogleContainerTools/distroless)
- [Docker BuildKit](https://docs.docker.com/build/buildkit/)
- [Layer Caching Best Practices](https://docs.docker.com/build/cache/)
