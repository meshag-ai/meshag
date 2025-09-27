# Docker Setup for Meshag Distributed Services

## 🚀 Quick Start

### Prerequisites
- Docker and Docker Compose installed
- OpenAI API key (required)
- Daily.co API key and domain (for transport service)
- Optional: ElevenLabs API key, Azure Speech API key

### 1. Set up environment variables

Create a `.env` file in the project root:
```bash
# Copy the example file (if available)
cp .env.example .env

# Or create manually with required variables:
cat > .env << EOF
# OpenAI Configuration (Required)
OPENAI_API_KEY=your-openai-api-key-here

# Daily.co Configuration (Required for Transport Service)
DAILY_API_KEY=your-daily-api-key-here
DAILY_DOMAIN=your-daily-domain-here

# ElevenLabs Configuration (Optional)
ELEVENLABS_API_KEY=your-elevenlabs-api-key-here

# Azure Speech Configuration (Optional)
AZURE_SPEECH_KEY=your-azure-speech-key-here
AZURE_SPEECH_REGION=your-azure-region-here

# Service Configuration
RUST_LOG=info
NATS_URL=nats://nats:4222
EOF
```

### 2. Build and run all services

```bash
# Navigate to docker directory
cd docker

# Build and start all services
docker-compose up -d

# Or build first then run
docker-compose build
docker-compose up -d
```

### 3. Verify services are running

```bash
# Check all containers
docker-compose ps

# Check logs for each service
docker-compose logs -f stt-service
docker-compose logs -f llm-service
docker-compose logs -f tts-service
docker-compose logs -f transport-service
docker-compose logs -f nats
```

## 🏗️ Architecture Overview

The system uses an **event-driven microservices architecture** with NATS JetStream for high-performance message streaming:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Client App    │────│ Transport Service │────│   Daily.co API  │
│                 │    │   (Port 8084)    │    │                 │
│ • Web/Mobile    │    │ • HTTP REST API  │    │ • WebRTC Rooms  │
│ • WebSocket     │    │ • WebSocket      │    │ • Meeting Tokens│
│ • Daily.co SDK  │    │ • Session Mgmt   │    │ • Presence API  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │
                                │ NATS JetStream
                                │
                    ┌───────────────────────┐
                    │   Processing Chain    │
                    │                       │
                    │ STT ──→ LLM ──→ TTS   │
                    │(8081)  (8082)  (8083)│
                    └───────────────────────┘
```

## 🔧 Service Endpoints

| Service | Port | Endpoints | Purpose |
|---------|------|-----------|---------|
| **STT Service** | 8081 | `http://localhost:8081/health` | Speech-to-Text with OpenAI Whisper |
| | | `http://localhost:8081/ready` | Readiness check |
| | | `http://localhost:8081/metrics` | Prometheus metrics |
| **LLM Service** | 8082 | `http://localhost:8082/health` | Language Model with OpenAI GPT |
| | | `http://localhost:8082/ready` | Readiness check |
| | | `http://localhost:8082/metrics` | Prometheus metrics |
| **TTS Service** | 8083 | `http://localhost:8083/health` | Text-to-Speech with ElevenLabs/Azure |
| | | `http://localhost:8083/ready` | Readiness check |
| | | `http://localhost:8083/metrics` | Prometheus metrics |
| **Transport Service** | 8084 | `http://localhost:8084/sessions` | WebRTC transport with Daily.co |
| | | `ws://localhost:8084/ws` | WebSocket for real-time events |
| | | `http://localhost:8084/health` | Health check |
| | | `http://localhost:8084/ready` | Readiness check |
| | | `http://localhost:8084/metrics` | Prometheus metrics |
| **NATS JetStream** | 4222 | `nats://localhost:4222` | High-performance message streaming |
| | 8222 | `http://localhost:8222` | NATS monitoring dashboard |

## 🧪 Testing Your Services

### Test Transport Service (WebRTC Sessions)

```bash
# Health check
curl http://localhost:8084/health

# Create a Daily.co session
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "daily",
    "room_config": {
      "privacy": "Private",
      "enable_recording": false,
      "enable_chat": true,
      "max_participants": 10
    },
    "participant_config": {
      "name": "Test User",
      "is_owner": true,
      "permissions": {
        "can_admin": true,
        "can_send_video": true,
        "can_send_audio": true,
        "can_send_screen_video": true,
        "can_send_screen_audio": true
      }
    },
    "options": {}
  }'

# List all sessions
curl http://localhost:8084/sessions

# WebSocket connection for real-time events
# (Use a WebSocket client like websocat)
websocat ws://localhost:8084/ws
```

### Test Processing Services

```bash
# Health checks
curl http://localhost:8081/health  # STT Service
curl http://localhost:8082/health  # LLM Service
curl http://localhost:8083/health  # TTS Service

# Readiness checks (shows connector status)
curl http://localhost:8081/ready   # Shows OpenAI Whisper status
curl http://localhost:8082/ready   # Shows OpenAI GPT status
curl http://localhost:8083/ready   # Shows ElevenLabs/Azure status

# Metrics (Prometheus format)
curl http://localhost:8081/metrics
curl http://localhost:8082/metrics
curl http://localhost:8083/metrics
curl http://localhost:8084/metrics
```

### Test NATS JetStream

```bash
# Check NATS server status
curl http://localhost:8222/healthz

# View NATS monitoring dashboard
open http://localhost:8222

# Connect to NATS CLI (if you have nats CLI installed)
nats --server=localhost:4222 stream ls
nats --server=localhost:4222 stream info audio.input
nats --server=localhost:4222 stream info stt.output
nats --server=localhost:4222 stream info llm.output
```

## 📊 Optional: Monitoring Stack

### Start with monitoring tools
```bash
# Start with Valkey Commander (Web UI for debugging)
docker-compose --profile tools up -d

# Start with Prometheus and Grafana
docker-compose --profile monitoring up -d

# Start everything including monitoring
docker-compose --profile tools --profile monitoring up -d
```

### Access monitoring tools
- **NATS Monitoring**: http://localhost:8222 (Built-in NATS dashboard)
- **Valkey Commander**: http://localhost:8082 (Web UI for Valkey - if enabled)
- **Prometheus**: http://localhost:9090 (Metrics collection)
- **Grafana**: http://localhost:3000 (admin/admin - Metrics visualization)

## 🛠️ Development Commands

### Build specific service
```bash
# Navigate to docker directory first
cd docker

# Build only STT service
docker-compose build stt-service

# Build only LLM service
docker-compose build llm-service

# Build only TTS service
docker-compose build tts-service

# Build only Transport service
docker-compose build transport-service
```

### Run specific services
```bash
# Run only core processing services
docker-compose up -d nats stt-service llm-service tts-service

# Run with transport service
docker-compose up -d nats stt-service llm-service tts-service transport-service

# Run with logs
docker-compose up nats stt-service llm-service tts-service transport-service
```

### Debugging
```bash
# View logs
docker-compose logs -f stt-service
docker-compose logs -f llm-service
docker-compose logs -f tts-service
docker-compose logs -f transport-service
docker-compose logs -f nats

# Execute into container
docker exec -it stt-service /bin/bash
docker exec -it llm-service /bin/bash
docker exec -it tts-service /bin/bash
docker exec -it transport-service /bin/bash

# Check service health
curl http://localhost:8081/ready
curl http://localhost:8082/ready
curl http://localhost:8083/ready
curl http://localhost:8084/ready
```

## 🔄 Development Workflow

### 1. Code changes
```bash
# After making code changes, rebuild and restart
cd docker
docker-compose build stt-service
docker-compose up -d stt-service

# Or rebuild everything
docker-compose build
docker-compose up -d
```

### 2. Clean restart
```bash
# Stop all services
docker-compose down

# Remove volumes (clears NATS data)
docker-compose down -v

# Rebuild and restart
docker-compose build
docker-compose up -d
```

### 3. View metrics and monitoring
```bash
# Service metrics (Prometheus format)
curl http://localhost:8081/metrics
curl http://localhost:8082/metrics
curl http://localhost:8083/metrics
curl http://localhost:8084/metrics

# NATS monitoring
curl http://localhost:8222/varz
curl http://localhost:8222/connz
curl http://localhost:8222/subsz

# View in Prometheus UI
open http://localhost:9090/targets
```

## 🐳 Production Considerations

### Environment Variables
```bash
# Production environment file
OPENAI_API_KEY=sk-your-production-key
DAILY_API_KEY=your-production-daily-key
DAILY_DOMAIN=your-production-domain
ELEVENLABS_API_KEY=your-production-elevenlabs-key
AZURE_SPEECH_KEY=your-production-azure-key
AZURE_SPEECH_REGION=your-azure-region
RUST_LOG=warn
NATS_URL=nats://nats:4222
```

### Resource Limits
Add to docker-compose.yml:
```yaml
services:
  stt-service:
    deploy:
      resources:
        limits:
          cpus: '1.0'
          memory: 512M
        reservations:
          cpus: '0.5'
          memory: 256M

  transport-service:
    deploy:
      resources:
        limits:
          cpus: '0.5'
          memory: 256M
        reservations:
          cpus: '0.25'
          memory: 128M
```

### Security
```yaml
services:
  stt-service:
    security_opt:
      - no-new-privileges:true
    read_only: true
    tmpfs:
      - /tmp

  transport-service:
    security_opt:
      - no-new-privileges:true
    read_only: true
    tmpfs:
      - /tmp
```

## 🚨 Troubleshooting

### Common Issues

**Services won't start**
```bash
# Check if ports are available
netstat -tulpn | grep :8081
netstat -tulpn | grep :8082
netstat -tulpn | grep :8083
netstat -tulpn | grep :8084
netstat -tulpn | grep :4222

# Check Docker logs
docker-compose logs stt-service
docker-compose logs llm-service
docker-compose logs tts-service
docker-compose logs transport-service
docker-compose logs nats
```

**NATS connection issues**
```bash
# Test NATS connectivity
curl http://localhost:8222/healthz

# Check NATS logs
docker-compose logs nats

# Test NATS from inside a service container
docker exec meshag-stt-service curl http://nats:8222/healthz
```

**API Key issues**
```bash
# Check environment variables are loaded
docker exec stt-service env | grep OPENAI
docker exec transport-service env | grep DAILY
docker exec tts-service env | grep ELEVENLABS

# Test API connectivity
docker exec meshag-stt-service curl -H "Authorization: Bearer $OPENAI_API_KEY" https://api.openai.com/v1/models
```

**Build failures**
```bash
# Clean build
docker-compose build --no-cache

# Check Rust compilation locally
cargo check --workspace

# Check specific service
cargo check -p stt-service
cargo check -p llm-service
cargo check -p tts-service
cargo check -p transport-service
```

**Daily.co integration issues**
```bash
# Test Daily.co API directly
curl -H "Authorization: Bearer $DAILY_API_KEY" https://api.daily.co/v1/

# Check transport service logs
docker-compose logs transport-service

# Test session creation
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{"provider": "daily", "room_config": {"privacy": "Private"}, "participant_config": {"name": "Test", "is_owner": true, "permissions": {"can_send_audio": true, "can_send_video": true, "can_admin": true, "can_send_screen_audio": true, "can_send_screen_video": true}}, "options": {}}'
```

## 📈 Scaling

### Horizontal scaling
```bash
# Scale processing services
docker-compose up -d --scale stt-service=3
docker-compose up -d --scale llm-service=2
docker-compose up -d --scale tts-service=2

# Scale transport service
docker-compose up -d --scale transport-service=2
```

### Load balancing
For production load balancing, add nginx or traefik:

```yaml
# Add to docker-compose.yml
nginx:
  image: nginx:alpine
  ports:
    - "80:80"
  volumes:
    - ./nginx.conf:/etc/nginx/nginx.conf
  depends_on:
    - stt-service
    - llm-service
    - tts-service
    - transport-service
```

## 🎯 Integration Examples

### Full Conversation Flow
1. **Create Transport Session**: POST to `/sessions` → Get Daily.co room URL
2. **Join Room**: Use Daily.co SDK to join the WebRTC room
3. **Audio Processing**: Audio flows through STT → LLM → TTS via NATS
4. **Real-time Updates**: WebSocket connection provides session status updates

### Event Flow
```
Audio Input → STT Service → NATS(stt.output) → LLM Service → NATS(llm.output) → TTS Service → Audio Output
                ↓                                    ↓                                    ↓
         NATS(audio.input)                  NATS(conversation)                 NATS(audio.output)
```

Your distributed Pipecat services with Daily.co WebRTC transport are now ready to run! 🎉

## 🔗 Related Documentation

- [Transport Service README](TRANSPORT_SERVICE_README.md) - Detailed transport service documentation
- [Daily.co API Documentation](https://docs.daily.co/reference/rest-api) - WebRTC provider documentation
- [NATS JetStream Documentation](https://docs.nats.io/jetstream) - Message streaming documentation
