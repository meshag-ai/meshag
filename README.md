# MeshAG - Distributed AI Agent Orchestrator

A high-performance, event-driven microservices architecture for orchestrating AI agents in real-time conversational applications with WebRTC transport.

## 🚀 Overview

MeshAG is a distributed AI agent orchestrator that enables seamless coordination of multiple AI services using **NATS JetStream** for ultra-low latency message streaming and **Daily.co** for WebRTC transport. Each AI agent operates independently with pluggable connectors for different AI providers, creating a scalable and flexible conversational AI platform.

### Architecture

```
┌─────────────────┐    ┌───────────────────┐    ┌──────────────────┐
│   Client App    │────│ Transport Service │────│   Daily.co API   │
│                 │    │   (Port 8084)     │    │                  │
│ • Web/Mobile    │    │ • HTTP REST API   │    │ • WebRTC Rooms   │
│ • WebSocket     │    │ • WebSocket       │    │ • Meeting Tokens │
│ • Daily.co SDK  │    │ • Session Mgmt    │    │ • Presence API   │
└─────────────────┘    └───────────────────┘    └──────────────────┘
                                │
                                │ NATS JetStream
                                │
                    ┌───────────────────────┐
                    │   AI Agent Chain      │
                    │                       │
                    │ STT ──→ LLM ──→ TTS   │
                    │(8081)  (8082)  (8083) │
                    └───────────────────────┘
```

## 🏗️ AI Agent Services

| Service | Port | Purpose | AI Providers |
|---------|------|---------|-------------|
| **STT Agent** | 8081 | Speech-to-Text | OpenAI Whisper, Deepgram |
| **LLM Agent** | 8082 | Language Model | OpenAI GPT, Anthropic Claude |
| **TTS Agent** | 8083 | Text-to-Speech | ElevenLabs, Azure Speech |
| **Transport Service** | 8084 | WebRTC Transport | Daily.co |

## ⚡ Key Features

- **Event-Driven Orchestration**: NATS JetStream for ultra-high-performance agent coordination
- **Independent AI Agents**: Each agent operates autonomously and queues events for the next
- **WebRTC Integration**: Real-time audio/video communication via Daily.co
- **Pluggable AI Providers**: Easy switching between AI providers (OpenAI, ElevenLabs, Azure, etc.)
- **Production Ready**: Comprehensive health checks, metrics, and Docker containerization
- **Horizontally Scalable**: Load balancing support for high-throughput applications

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

### 3. Verify
```bash
# Check all AI agents are healthy
curl http://localhost:8081/health  # STT Agent
curl http://localhost:8082/health  # LLM Agent
curl http://localhost:8083/health  # TTS Agent
curl http://localhost:8084/health  # Transport Service
```

### 4. Create a Session
```bash
# Create Daily.co WebRTC session
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "daily",
    "room_config": {
      "privacy": "Private",
      "enable_chat": true
    },
    "participant_config": {
      "name": "User",
      "is_owner": true,
      "permissions": {
        "can_send_audio": true,
        "can_send_video": true,
        "can_admin": true,
        "can_send_screen_audio": true,
        "can_send_screen_video": true
      }
    },
    "options": {}
  }'
```

## 🛠️ Development

### Local Development
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Check compilation
cargo check --workspace

# Run individual AI agents
cargo run -p stt-service
cargo run -p llm-service
cargo run -p tts-service
cargo run -p meshag-transport-service
```

### Adding New AI Providers
1. Implement the connector trait (`SttConnector`, `LlmConnector`, or `TtsConnector`)
2. Add to the appropriate provider module
3. Register in the agent's main.rs
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
    stt_agent.register_connector("newprovider", connector).await;
}
```

## 📊 Monitoring

### Health Checks
- `/health` - AI agent health status
- `/ready` - Readiness with AI provider status
- `/metrics` - Prometheus metrics

### NATS Monitoring
- **Dashboard**: http://localhost:8222
- **Streams**: `audio.input`, `stt.output`, `llm.output`

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
| `RUST_LOG` | No | Log level (default: info) |

### Agent Ports
- **STT Agent**: 8081
- **LLM Agent**: 8082
- **TTS Agent**: 8083
- **Transport Service**: 8084
- **NATS**: 4222 (client), 8222 (monitoring)


## 📚 Documentation

- **[Docker Setup](DOCKER_SETUP.md)** - Complete Docker deployment guide
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
