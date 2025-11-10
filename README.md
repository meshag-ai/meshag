# Meshag - Distributed Voice AI Framework

A high-performance, distributed voice AI framework built in Rust with Python bindings. Inspired by Pipecat, Meshag provides a frame-based architecture for building real-time voice applications with custom processors, pipelines, and distributed microservices.

## 🚀 Features

- **Frame-Based Architecture**: Three-tier frame system (System, Control, Data) with priority processing
- **Distributed by Design**: Microservices communicate via NATS JetStream for reliable, scalable processing
- **WebSocket Gateway**: Real-time bidirectional communication with clients
- **Multi-Session Support**: Handle multiple concurrent voice conversations simultaneously
- **Custom Processors**: Extend functionality with local (in-process) or remote (WebSocket) processors
- **Python & Rust**: Write custom processors in either Python or Rust
- **Priority Interrupts**: System frames bypass queues for immediate handling (e.g., user interruptions)
- **Built-in Services**: STT, LLM, TTS, and Transport services included

## 📦 Architecture

```
┌─────────────────┐
│   API Gateway   │  WebSocket Server (Port 8080)
│  (meshag-service│  Routes frames to NATS
│   TYPE=gateway) │
└────────┬────────┘
         │
    ┌────▼─────┐
    │   NATS   │  Message Queue & Streaming
    │JetStream │  - SYSTEM.session.{id} (Priority)
    │          │  - DATA.session.{id} (Standard)
    └────┬─────┘
         │
    ┌────┴────────────────────────────┐
    │                                 │
┌───▼────┐  ┌────▼───┐  ┌────▼───┐  ┌───────▼───┐
│  STT   │  │  LLM   │  │  TTS   │  │ Transport │
│Service │  │Service │  │Service │  │  Service  │
└────────┘  └────────┘  └────────┘  └───────────┘
```

## 🏗️ Frame System

Meshag uses a three-tier frame hierarchy:

### System Frames (Priority 0)
Processed immediately, bypass queues:
- `StartFrame` - Session initialization
- `EndFrame` - Session termination
- `StartInterruptionFrame` - User interrupted bot
- `StopInterruptionFrame` - Interruption complete
- `ErrorFrame` - Error conditions
- `UserStartedSpeakingFrame` / `UserStoppedSpeakingFrame`
- `CancelFrame` - Cancel current operation

### Control Frames (Priority 1)
Queued control signals:
- `TTSStartedFrame` / `TTSStoppedFrame`
- `LLMFullResponseStartFrame` / `LLMFullResponseEndFrame`
- `BotStartedSpeakingFrame` / `BotStoppedSpeakingFrame`

### Data Frames (Priority 2)
Content data:
- `TextFrame` - Text messages
- `AudioFrame` - Audio data (PCM16, Opus, etc.)
- `TranscriptionFrame` - STT output with confidence scores
- `LLMTextFrame` - LLM responses
- `ImageFrame` / `VideoFrame` - Visual content

## 🎯 Quick Start

### Option 1: Docker (Recommended - 2 minutes)

**Prerequisites:** Docker and Docker Compose installed

```bash
# 1. Clone the repository
git clone https://github.com/yourusername/meshag.git
cd meshag

# 2. Create .env file with your API keys
cp .env.example .env
# Edit .env and add your API keys:
# - OPENAI_API_KEY=sk-...
# - DEEPGRAM_API_KEY=...
# - ELEVENLABS_API_KEY=...

# 3. Start everything (NATS + all services)
cd docker
docker-compose up -d

# 4. Check status
docker-compose ps

# 5. View logs
docker-compose logs -f gateway
```

**That's it!** Gateway is now running on `http://localhost:8080`

Test the WebSocket connection:
```bash
# Install wscat if you don't have it
npm install -g wscat

# Connect
wscat -c ws://localhost:8080/ws
```

### Option 2: Local Development (with Docker NATS)

**Prerequisites:** Rust 1.75+, Docker

```bash
# 1. Start NATS in Docker
docker run -d --name nats -p 4222:4222 -p 8222:8222 nats:2.10-alpine -js

# 2. Clone and build
git clone https://github.com/yourusername/meshag.git
cd meshag
cargo build --release --bin meshag-service

# 3. Set up environment
cp .env.example .env
# Edit .env with your API keys

# 4. Run services in separate terminals
# Terminal 1: Gateway
SERVICE_TYPE=gateway ./target/release/meshag-service

# Terminal 2: LLM
SERVICE_TYPE=llm ./target/release/meshag-service

# Terminal 3: STT
SERVICE_TYPE=stt ./target/release/meshag-service

# Terminal 4: TTS
SERVICE_TYPE=tts ./target/release/meshag-service
```

### Option 3: Build from Source (No Docker)

**Prerequisites:** Rust 1.75+, NATS Server

```bash
# 1. Install NATS
# macOS:
brew install nats-server

# Linux:
curl -L https://github.com/nats-io/nats-server/releases/download/v2.10.7/nats-server-v2.10.7-linux-amd64.tar.gz | tar -xz
sudo mv nats-server-v2.10.7-linux-amd64/nats-server /usr/local/bin/

# Windows:
# Download from https://github.com/nats-io/nats-server/releases

# 2. Start NATS with JetStream
nats-server -js

# 3. Build and run (same as Option 2, steps 2-4)
```

### Test Your Setup

**JavaScript Client:**
```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  console.log('Connected!');

  // Send a text frame
  ws.send(JSON.stringify({
    frame_category: "Data",
    Data: {
      TextFrame: {
        session_id: "test-session",
        frame_id: crypto.randomUUID(),
        timestamp: Date.now(),
        text: "Hello, AI!",
        language: "en"
      }
    }
  }));
};

ws.onmessage = (event) => {
  const frame = JSON.parse(event.data);
  console.log('Received frame:', frame.frame_category);
};

ws.onerror = (error) => console.error('WebSocket error:', error);
```

**cURL Health Check:**
```bash
# Check if gateway is running
curl http://localhost:8080/health

# Expected response:
# {"status":"healthy","service":"api-gateway"}
```

**NATS Monitor:**
```bash
# View NATS dashboard
open http://localhost:8222

# Or check with curl
curl http://localhost:8222/varz
```

## 🐍 Python Bindings

```bash
# Build Python package
cd crates/meshag-python
pip install maturin
maturin develop

# Use in Python
import meshag

# Create a pipeline
pipeline = meshag.Pipeline("my-pipeline")
runner = meshag.Runner(pipeline)

# Create frames
text_frame = meshag.TextFrame(
    session_id="session-1",
    text="Hello from Python!"
)
```

## 🔧 Configuration

Create a `.env` file:

```env
# Service type (gateway, llm, stt, tts, transport)
SERVICE_TYPE=gateway

# API Gateway
PORT=8080

# NATS
NATS_URL=nats://localhost:4222

# LLM Service
OPENAI_API_KEY=sk-your-key
OPENAI_BASE_URL=https://api.openai.com/v1
DEFAULT_MODEL=gpt-4

# STT Service
DEEPGRAM_API_KEY=your-key

# TTS Service
ELEVENLABS_API_KEY=your-key

# Transport Service
DAILY_API_KEY=your-key
DAILY_ROOM_URL=https://yourdomain.daily.co/room
```

## 🧪 Testing

```bash
# Run all tests
cargo test --workspace

# Run specific test suites
cargo test --package meshag-shared --test frames_test
cargo test --package meshag-orchestrator --test pipeline_test
cargo test --package meshag-orchestrator --test runner_test
cargo test --test integration_test
cargo test --test multi_session_test

# Test coverage: 26+ tests covering:
# - Frame serialization (JSON/bincode)
# - Priority queue ordering
# - Pipeline processor chains
# - Runner lifecycle and interrupts
# - Multi-session concurrency
# - ProcessingEvent ↔ Frame conversion
```

## 📚 Project Structure

```
meshag/
├── crates/
│   ├── shared/              # Core types (frames, processor trait)
│   ├── orchestrator/        # Pipeline & Runner
│   ├── services/
│   │   ├── llm/            # LLM service implementation
│   │   ├── stt/            # Speech-to-text service
│   │   ├── tts/            # Text-to-speech service
│   │   └── transport/      # WebRTC transport
│   ├── meshag-python/       # Python bindings (PyO3)
│   └── connectors/         # External API connectors
├── services/
│   └── meshag-service/     # Unified service binary
├── meshag/                  # Python package
└── tests/                   # Integration tests
```

## 🔌 Custom Processors

### Rust Local Processor

```rust
use meshag_orchestrator::{Pipeline, PipelineBuilder};
use meshag_shared::{DataFrame, FrameWrapper};

let pipeline = PipelineBuilder::new("my-pipeline")
    .add_local_fn("uppercase", |frame| {
        Box::pin(async move {
            if let FrameWrapper::Data(DataFrame::TextFrame { text, .. }) = frame {
                let upper = text.to_uppercase();
                Ok(vec![FrameWrapper::Data(DataFrame::TextFrame {
                    // ... modified frame
                })])
            } else {
                Ok(vec![frame])
            }
        })
    })
    .build();
```

### Remote Processor (WebSocket)

```rust
use meshag_shared::RemoteProcessor;

let processor = RemoteProcessor::new(
    "sentiment-analyzer",
    "ws://localhost:9000/process"
);

pipeline.add_processor(Box::new(processor));
```

## 🐳 Docker Deployment

All Docker files are in the `docker/` directory:
- **`docker/Dockerfile`** - Multi-stage build optimized for size
- **`docker/docker-compose.yml`** - Complete stack with NATS, Valkey, and all services

### Quick Docker Start

```bash
# From project root
cd docker
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

### Or Use Makefile

```bash
# From project root
make docker-up       # Start everything
make docker-down     # Stop everything
make docker-logs     # View logs
make docker-rebuild  # Rebuild images and restart
```

### What's Included

The docker-compose stack includes:
- **NATS** - Message streaming (ports 4222, 8222)
- **Valkey** - Redis-compatible config store (port 6379)
- **Gateway** - API Gateway service (port 8080)
- **LLM** - Language model service (port 8082)
- **STT** - Speech-to-text service (port 8081)
- **TTS** - Text-to-speech service (port 8083)
- **Transport** - WebRTC transport (overlaps with gateway)

## 🎨 Key Concepts

### Multi-Session Dispatcher Pattern

Each service uses a dispatcher to handle multiple concurrent sessions:

```rust
// Central dispatcher routes frames to per-session handlers
pub struct MultiSessionLlmService {
    llm_service: Arc<LlmService>,
    sessions: Arc<DashMap<String, SessionChannels>>,
}

// Per-session handler with cancellation support
tokio::select! {
    biased;

    Some(frame) = system_rx.recv() => {
        // Process system frames immediately
        match frame {
            SystemFrame::StartInterruptionFrame => {
                cancel_token.cancel(); // Stop current processing
            }
            _ => {}
        }
    }

    Some(frame) = data_rx.recv() => {
        // Process data frames (can be interrupted)
    }
}
```

### Frame Priority

```rust
pub enum FrameCategory {
    System = 0,  // Highest priority
    Control = 1, // Medium priority
    Data = 2,    // Standard priority
}

// Runner uses BinaryHeap to ensure system frames process first
let mut queue = BinaryHeap::new();
queue.push(PrioritizedFrame { priority: 0, frame: system_frame });
queue.push(PrioritizedFrame { priority: 2, frame: data_frame });

// System frame pops first
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### CI/CD Status

![Tests](https://github.com/meshag-ai/meshag/workflows/Tests/badge.svg)
![Build and Release](https://github.com/meshag-ai/meshag/workflows/Build%20and%20Release%20Images/badge.svg)

#### Automated Workflows

1. **Tests** (`test.yml`) - Runs automatically on every push/PR
   - Code formatting check (`cargo fmt`)
   - Linting (`cargo clippy`)
   - Unit & integration tests (`cargo test`)
   - Security audit (`cargo audit`)

2. **Build and Release** (`build-and-release.yml`) - Manual trigger only
   - Multi-platform Docker builds (amd64, arm64)
   - Security scanning with Trivy
   - Push to GitHub Container Registry
   - Auto-update docker-compose.yml

All pull requests must pass:
- ✅ Code formatting (`cargo fmt`)
- ✅ Clippy lints (`cargo clippy`)
- ✅ Unit and integration tests (`cargo test`)
- ✅ Security audit (`cargo audit`)

## 📄 License

[Your License Here]

## 🙏 Acknowledgments

- Inspired by [Pipecat](https://github.com/pipecat-ai/pipecat)
- Built with [Rust](https://www.rust-lang.org/) and [Tokio](https://tokio.rs/)
- Uses [NATS](https://nats.io/) for messaging

## 📞 Support

- GitHub Issues: [Your Issues URL]
- Documentation: [Your Docs URL]
- Discord: [Your Discord URL]

---

**Built with ❤️ by [abskrj](https://github.com/abskrj)**
