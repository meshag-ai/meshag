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
| **STT Service** | 8081 | Speech-to-Text | Deepgram | Valkey + Self-routing |
| **LLM Service** | 8082 | Language Model | OpenAI GPT | Valkey + Self-routing |
| **TTS Service** | 8083 | Text-to-Speech | ElevenLabs | Valkey + Self-routing |
| **Transport Service** | 8084 | WebRTC Transport | Twilio | Valkey + Session Management |

## ⚡ Key Features

- **Event-Driven Orchestration**: NATS JetStream for ultra-high-performance agent coordination
- **Valkey Configuration Storage**: Redis-compatible key-value store for dynamic configuration management (**under dev**)
- **Self-Routing Services**: Each service determines its next consumer based on stored configuration
- **WebRTC Integration**: Real-time audio/video communication via Daily.co
- **Pluggable AI Providers**: Easy switching between AI providers (OpenAI, ElevenLabs, etc.)
- **Production Ready**: Comprehensive health checks, metrics, and Docker containerization
- **Horizontally Scalable**: Load balancing support for high-throughput applications
- **TTL-based Configuration**: Automatic cleanup of expired session configurations

## 🚀 Quick Start

### Prerequisites
- Docker and Docker Compose
- OpenAI API key
- ElevenLabs API key
- Deepgram API Key

### 1. Environment Setup
```bash
# Create environment file
cat > .env << EOF
OPENAI_API_KEY=your-openai-api-key-here
ELEVENLABS_API_KEY=your-elevenlabs-api-key-here
DEEPGRAM_API_KEY=your-deepgram-api-key-here
NATS_URL=nats://localhost:4222
VALKEY_URL=redis://localhost:6379
EOF
```

### 2. Start Services
```bash
make up
```

### 3. Setup Ngrok to hit port Transport servcie's port (8080/8084)
```bash
https://semiadhesive-stephane-uninchoative.ngrok-free.dev -> http://localhost:8080
```

### 4. Setup Twilio to hit endpoint /twilio



## 🛠️ Development

### Local Development (Docker Required)
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

make infra

# in seperate terminals
make t

# in seperate terminals
make s

# in seperate terminals
make l

# in seperate terminals
make tt
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


### Service Ports
- **STT Service**: 8081
- **LLM Service**: 8082
- **TTS Service**: 8083
- **Transport Service**: 8084/8080
- **NATS**: 4222 (client), 8222 (monitoring)
- **Valkey**: 6379

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
5. Run `cargo check --workspace`
6. Submit a pull request

---

**Built with ❤️ using Rust by [abskrj](https://github.com/abskrj)**
