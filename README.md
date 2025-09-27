# MeshAG - Distributed AI Agent Orchestrator

A high-performance, event-driven microservices architecture for orchestrating AI agents in real-time conversational applications with WebRTC transport and Valkey-based configuration management.

## 🚀 Overview

MeshAG is a distributed AI agent orchestrator that enables seamless coordination of multiple AI services using **NATS JetStream** for ultra-low latency message streaming, **Valkey** for configuration storage, and **Daily.co** for WebRTC transport. Each AI agent operates independently with pluggable connectors for different AI providers, creating a scalable and flexible conversational AI platform.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           MeshAG Distributed AI Orchestrator                    │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────┐    ┌───────────────────┐    ┌──────────────────┐
│   Client App    │────│ Transport Service │────│   Daily.co API   │
│                 │    │   (Port 8084)     │    │                  │
│ • Web/Mobile    │    │ • HTTP REST API   │    │ • WebRTC Rooms   │
│ • WebSocket     │    │ • WebSocket       │    │ • Meeting Tokens │
│ • Daily.co SDK  │    │ • Session Mgmt    │    │ • Presence API   │
└─────────────────┘    └───────────────────┘    └──────────────────┘
                                │
                                │ HTTP POST /sessions/with-config
                                │ (JSON Configuration)
                                ▼
                    ┌───────────────────────┐
                    │       Valkey          │
                    │   (Port 6379)         │
                    │ • Config Storage      │
                    │ • Session Management  │
                    │ • TTL-based Cleanup   │
                    └───────────────────────┘
                                ▲
                                │ Config Retrieval
                                │
                    ┌─────────────────────────────────────────┐
                    │           NATS JetStream                │
                    │            (Port 4222)                  │
                    │         • Event Streaming               │
                    │         • Service Communication         │
                    │         • Ultra-low Latency             │
                    └─────────────────────────────────────────┘
                                ▲
                                │ Event Routing
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
┌─────────────┐        ┌─────────────┐        ┌─────────────┐
│ STT Service │        │ LLM Service │        │ TTS Service │
│ (Port 8081) │        │ (Port 8082) │        │ (Port 8083) │
│             │        │             │        │             │
│ • OpenAI    │        │ • OpenAI    │        │ • ElevenLabs│
│ • Deepgram  │        │ • Anthropic │        │ • Azure     │
│ • Self-route│        │ • Self-route│        │ • Self-route│
└─────────────┘        └─────────────┘        └─────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │   Pipeline Flow       │
                    │                       │
                    │ transport → stt       │
                    │ stt → llm             │
                    │ llm → tts             │
                    │ tts → transport       │
                    └───────────────────────┘
```

## 🏗️ AI Agent Services

| Service | Port | Purpose | AI Providers | Configuration |
|---------|------|---------|-------------|---------------|
| **STT Service** | 8081 | Speech-to-Text | OpenAI Whisper, Deepgram | Valkey + Self-routing |
| **LLM Service** | 8082 | Language Model | OpenAI GPT, Anthropic Claude | Valkey + Self-routing |
| **TTS Service** | 8083 | Text-to-Speech | ElevenLabs, Azure Speech | Valkey + Self-routing |
| **Transport Service** | 8084 | WebRTC Transport | Daily.co | Valkey + Session Management |

## ⚡ Key Features

- **Event-Driven Orchestration**: NATS JetStream for ultra-high-performance agent coordination
- **Valkey Configuration Storage**: Redis-compatible key-value store for dynamic configuration management
- **Self-Routing Services**: Each service determines its next consumer based on stored configuration
- **WebRTC Integration**: Real-time audio/video communication via Daily.co
- **Pluggable AI Providers**: Easy switching between AI providers (OpenAI, ElevenLabs, Azure, etc.)
- **Production Ready**: Comprehensive health checks, metrics, and Docker containerization
- **Horizontally Scalable**: Load balancing support for high-throughput applications
- **TTL-based Configuration**: Automatic cleanup of expired session configurations

## 🚀 Quick Start

### Prerequisites
- Docker and Docker Compose
- OpenAI API key
- Daily.co API key and domain

### 1. Environment Setup
```bash
# Create environment file
cat > .env << EOF
OPENAI_API_KEY=your-openai-api-key-here
DAILY_API_KEY=your-daily-api-key-here
DAILY_DOMAIN=your-daily-domain-here
ELEVENLABS_API_KEY=your-elevenlabs-api-key-here  # Optional
AZURE_SPEECH_KEY=your-azure-speech-key-here      # Optional
AZURE_SPEECH_REGION=your-azure-region-here       # Optional
EOF
```

### 2. Start Services
```bash
cd docker
docker-compose up -d
```

### 3. Verify Services
```bash
# Check all services are healthy
curl http://localhost:8081/health  # STT Service
curl http://localhost:8082/health  # LLM Service
curl http://localhost:8083/health  # TTS Service
curl http://localhost:8084/health  # Transport Service

# Check Valkey
redis-cli -h localhost -p 6379 ping  # Should return PONG

# Check NATS
curl http://localhost:8222/healthz  # NATS monitoring
```

### 4. Create a Session with Configuration
```bash
# Create Daily.co WebRTC session with AI agent configuration
curl -X POST http://localhost:8084/sessions/with-config \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "system_prompt": "You are a helpful AI assistant that provides clear, concise, and accurate responses. Keep your responses conversational and natural for voice interaction.",
      "pipeline": {
        "name": "voice_chat",
        "flow": [
          {"from": "transport", "to": "stt", "stream": "audio"},
          {"from": "stt", "to": "llm", "stream": "text"},
          {"from": "llm", "to": "tts", "stream": "text"},
          {"from": "tts", "to": "transport", "stream": "audio"}
        ]
      },
      "connectors": {
        "stt": {
          "primary": "openai",
          "fallback": ["deepgram"],
          "config": {
            "openai": {
              "model": "whisper-1",
              "language": "auto",
              "response_format": "json",
              "temperature": 0
            }
          }
        },
        "llm": {
          "primary": "openai",
          "fallback": ["anthropic"],
          "config": {
            "openai": {
              "model": "gpt-4o",
              "temperature": 0.7,
              "max_tokens": 1000
            }
          }
        },
        "tts": {
          "primary": "elevenlabs",
          "fallback": ["azure"],
          "config": {
            "elevenlabs": {
              "model": "eleven_multilingual_v2",
              "voice": "Rachel",
              "stability": 0.5,
              "similarity_boost": 0.8
            }
          }
        },
        "transport": {
          "primary": "daily",
          "config": {}
        }
      }
    },
    "session_name": "my-ai-session",
    "max_participants": 10,
    "enable_recording": true,
    "enable_transcription": true
  }'
```

**Expected Response:**
```json
{
  "session_id": "abc123-def456-ghi789",
  "room_url": "https://your-domain.daily.co/abc123-def456-ghi789",
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "config_stored": true
}
```

## 🛠️ Development

### Local Development
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Check compilation
cargo check --workspace

# Run individual services
cargo run -p stt-service
cargo run -p llm-service
cargo run -p tts-service
cargo run -p transport-service
```

### Adding New AI Providers
1. Implement the connector trait (`SttConnector`, `LlmConnector`, or `TtsConnector`)
2. Add to the appropriate provider module
3. Register in the service's main.rs
4. Update environment variables

Example:
```rust
// In crates/connectors/src/providers/newprovider.rs
pub struct NewProviderSttConnector { /* ... */ }

#[async_trait]
impl SttConnector for NewProviderSttConnector {
    // Implement required methods
}

// In services/stt-service/src/main.rs
if let Ok(api_key) = std::env::var("NEWPROVIDER_API_KEY") {
    let config = NewProviderConfig::new(api_key);
    let connector = NewProvider::stt_connector(config);
    stt_service.register_connector("newprovider", connector).await;
}
```

## 📊 Monitoring

### Health Checks
- `/health` - Service health status
- `/ready` - Readiness with AI provider status
- `/metrics` - Prometheus metrics

### Service Monitoring
- **NATS Dashboard**: http://localhost:8222
- **Valkey CLI**: `redis-cli -h localhost -p 6379`
- **Streams**: `audio.input`, `stt.output`, `llm.output`, `tts.output`

### Optional Monitoring Stack
```bash
# Start with Prometheus and Grafana
docker-compose --profile monitoring up -d

# Access dashboards
open http://localhost:9090  # Prometheus
open http://localhost:3000  # Grafana (admin/admin)
```

## 🔧 Configuration

### Environment Variables
| Variable | Required | Description |
|----------|----------|-------------|
| `OPENAI_API_KEY` | Yes | OpenAI API key for STT/LLM |
| `DAILY_API_KEY` | Yes | Daily.co API key |
| `DAILY_DOMAIN` | Yes | Daily.co domain |
| `ELEVENLABS_API_KEY` | No | ElevenLabs TTS API key |
| `AZURE_SPEECH_KEY` | No | Azure Speech API key |
| `AZURE_SPEECH_REGION` | No | Azure region |
| `NATS_URL` | No | NATS server URL (default: nats://localhost:4222) |
| `VALKEY_URL` | No | Valkey server URL (default: redis://localhost:6379) |
| `RUST_LOG` | No | Log level (default: info) |

### Service Ports
- **STT Service**: 8081
- **LLM Service**: 8082
- **TTS Service**: 8083
- **Transport Service**: 8084
- **NATS**: 4222 (client), 8222 (monitoring)
- **Valkey**: 6379

## 🔄 Configuration Flow

1. **Client** sends JSON configuration to Transport Service via HTTP POST
2. **Transport Service** validates and stores configuration in Valkey with session ID
3. **All Services** read configuration from Valkey on startup using session ID
4. **Services** use orchestrator logic to determine next consumer in pipeline
5. **Pipeline** executes: transport → stt → llm → tts → transport

## 📚 Documentation

- **[Docker Setup](DOCKER_SETUP.md)** - Complete Docker deployment guide
- **[Configuration Guide](CONFIG_README.md)** - Valkey-based configuration system
- **[Transport Service](TRANSPORT_SERVICE_README.md)** - WebRTC transport documentation
- **[Daily.co API](https://docs.daily.co/reference/rest-api)** - WebRTC provider docs

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run `cargo check --workspace`
6. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

---

**Built with ❤️ using Rust by [abskrj](https://github.com/abskrj)**
