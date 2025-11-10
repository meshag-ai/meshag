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

### Prerequisites

- Rust 1.75+ (for building)
- NATS Server (for message queue)
- Python 3.8+ (optional, for Python bindings)

### 1. Install NATS

```bash
# macOS
brew install nats-server

# Linux
curl -L https://github.com/nats-io/nats-server/releases/download/v2.10.7/nats-server-v2.10.7-linux-amd64.tar.gz | tar -xz
sudo mv nats-server-v2.10.7-linux-amd64/nats-server /usr/local/bin/

# Start NATS
nats-server
```

### 2. Build Meshag

```bash
# Clone the repository
git clone https://github.com/yourusername/meshag.git
cd meshag

# Build all services (single binary!)
cargo build --release --bin meshag-service

# Binary location: target/release/meshag-service
```

### 3. Run Services

The `meshag-service` binary includes ALL services. Use `SERVICE_TYPE` to select which to run:

```bash
# Terminal 1: Start API Gateway
SERVICE_TYPE=gateway target/release/meshag-service

# Terminal 2: Start LLM Service
SERVICE_TYPE=llm OPENAI_API_KEY=your_key target/release/meshag-service

# Terminal 3: Start STT Service
SERVICE_TYPE=stt DEEPGRAM_API_KEY=your_key target/release/meshag-service

# Terminal 4: Start TTS Service
SERVICE_TYPE=tts ELEVENLABS_API_KEY=your_key target/release/meshag-service
```

### 4. Connect via WebSocket

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Send a text frame
ws.send(JSON.stringify({
  frame_category: "Data",
  Data: {
    TextFrame: {
      session_id: "my-session",
      frame_id: crypto.randomUUID(),
      timestamp: Date.now(),
      text: "Hello, AI!",
      language: "en"
    }
  }
}));

// Receive frames
ws.onmessage = (event) => {
  const frame = JSON.parse(event.data);
  console.log('Received:', frame);
};
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

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin meshag-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/meshag-service /usr/local/bin/
CMD ["meshag-service"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  nats:
    image: nats:latest
    ports:
      - "4222:4222"

  gateway:
    build: .
    environment:
      SERVICE_TYPE: gateway
      PORT: 8080
      NATS_URL: nats://nats:4222
    ports:
      - "8080:8080"
    depends_on:
      - nats

  llm:
    build: .
    environment:
      SERVICE_TYPE: llm
      NATS_URL: nats://nats:4222
      OPENAI_API_KEY: ${OPENAI_API_KEY}
    depends_on:
      - nats
```

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

**Built with ❤️ using Rust**
