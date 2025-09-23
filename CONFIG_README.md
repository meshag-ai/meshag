# Distributed AI Agent Orchestrator - Configuration System

## Overview

The Distributed AI Agent Orchestrator uses a simplified JSON configuration system that defines:
- **System prompt** for the LLM
- **Pipeline flow** between services
- **Connector configurations** for each service
- **NATS-based configuration storage** and routing

## Configuration Structure

### Basic Configuration (`config/agent-config.json`)

```json
{
  "system_prompt": "You are a helpful AI assistant...",
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
      "config": { /* connector-specific config */ }
    },
    "llm": {
      "primary": "openai",
      "fallback": ["anthropic"],
      "config": { /* connector-specific config */ }
    },
    "tts": {
      "primary": "elevenlabs",
      "fallback": ["azure"],
      "config": { /* connector-specific config */ }
    },
    "transport": {
      "primary": "daily",
      "config": { /* connector-specific config */ }
    }
  }
}
```

## Key Features

### 1. **Simplified Structure**
- Single system prompt (no multiple variants)
- No service endpoints (NATS-only communication)
- Pipeline-centric configuration
- Minimal, focused design

### 2. **Valkey Configuration Storage**
- Configurations stored in Valkey (Redis-compatible) with TTL
- Transport service acts as config manager
- All services read config from Valkey on startup
- Dynamic config updates without service restarts

### 3. **Self-Routing Services**
- Each service uses orchestrator logic to determine next consumer
- Event-driven routing based on stored configuration
- No separate orchestrator service needed

### 4. **Pipeline Flow**
- Linear pipeline: `transport → stt → llm → tts → transport`
- Each step defines source, target, and stream type
- Services automatically route to next step in pipeline

## Usage

### Loading Configuration

```rust
use meshag_orchestrator::AgentOrchestrator;

// Load from file
let orchestrator = AgentOrchestrator::from_config_file("config/agent-config.json").await?;

// Load from JSON string
let orchestrator = AgentOrchestrator::from_json(json_string)?;

// Validate configuration
orchestrator.validate_config()?;
```

### Storing Configuration in Valkey

```rust
use meshag_orchestrator::ConfigStorage;

let config_storage = ConfigStorage::new("redis://localhost:6379").await?;

// Store configuration for a session
config_storage.store_config("session_123", &orchestrator.config).await?;

// Retrieve configuration
let config = config_storage.get_config("session_123").await?;

// List all configurations
let configs = config_storage.list_configs().await?;

// Get TTL for configuration
let ttl = config_storage.get_config_ttl("session_123").await?;

// Extend TTL
config_storage.extend_config_ttl("session_123", 7200).await?; // 2 hours

// Clean up expired configurations
let cleaned = config_storage.cleanup_expired_configs().await?;

// Delete configuration
config_storage.delete_config("session_123").await?;
```

### Service Routing

```rust
use meshag_orchestrator::ServiceRouter;

// Create router for a service
let mut router = ServiceRouter::new(
    "redis://localhost:6379",
    event_queue,
    "stt".to_string(),
).await?;

// Load configuration for session
router.load_config("session_123").await?;

// Route output to next service
router.route_to_next(payload, "session_123").await?;

// Get connector configuration
let connector_config = router.get_connector_config();

// Get system prompt
let system_prompt = router.get_system_prompt();
```

## Pipeline Flow Example

```
1. Transport Service receives audio input
   ↓ (stores config in NATS)

2. STT Service processes audio
   ↓ (reads config, routes to LLM)

3. LLM Service processes text
   ↓ (reads config, routes to TTS)

4. TTS Service generates audio
   ↓ (reads config, routes to Transport)

5. Transport Service sends audio output
```

## Configuration Management

### Transport Service (Config Manager)
- Stores configurations in Valkey (Redis-compatible)
- Manages session-to-config mapping
- Provides config CRUD operations
- Handles config TTL and cleanup

### Individual Services
- Read configuration from Valkey on startup
- Use orchestrator logic for routing decisions
- Self-route based on pipeline flow
- No external orchestration needed

## Benefits

1. **Simplified**: No complex service endpoints or multiple prompts
2. **Self-contained**: Each service knows its routing
3. **Dynamic**: Config changes without restarts
4. **Scalable**: Services can be scaled independently
5. **Fault-tolerant**: Fallback connectors per service
6. **Event-driven**: Pure NATS-based communication with Valkey config storage

## Example Usage

See `examples/valkey-config-usage.rs` for a complete working example.

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Transport      │    │  STT Service    │    │  LLM Service    │
│  Service        │───▶│                 │───▶│                 │
│  (Config Mgmt)  │    │  (Self-routing) │    │  (Self-routing) │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         ▲                       │                       │
         │                       ▼                       ▼
         │              ┌─────────────────┐    ┌─────────────────┐
         └──────────────│  TTS Service    │◀───│                 │
                        │                 │    │                 │
                        │  (Self-routing) │    │                 │
                        └─────────────────┘                            └─────────────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │     Valkey      │
                        │  (Config Store) │
                        └─────────────────┘
```

This architecture provides a clean, efficient, and scalable way to orchestrate AI services with minimal configuration overhead.
