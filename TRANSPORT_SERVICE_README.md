# Transport Service with Daily.co Integration

## Overview

The Transport Service provides a WebRTC transport layer using Daily.co for real-time communication. It offers both HTTP REST API and WebSocket endpoints for session management and real-time communication.

## Architecture

### Components

1. **Transport Service** (`services/transport-service/`)
   - HTTP REST API for session management
   - WebSocket support for real-time communication
   - Daily.co integration for WebRTC rooms

2. **Daily.co Connector** (`crates/connectors/src/providers/daily.rs`)
   - Implements the `TransportConnector` trait
   - Manages Daily.co rooms and meeting tokens
   - Handles participant permissions and room configuration

3. **Transport Traits** (`crates/connectors/src/transport.rs`)
   - Generic `TransportConnector` trait
   - Common types for session management
   - Provider-agnostic interface

## Features

### HTTP Endpoints

- `POST /sessions` - Create a new session/room
- `GET /sessions` - List all sessions (with optional status filtering)
- `GET /sessions/{id}` - Get session details
- `DELETE /sessions/{id}` - End a session
- `GET /ws` - WebSocket upgrade endpoint
- `GET /health` - Health check
- `GET /ready` - Readiness check
- `GET /metrics` - Prometheus metrics

### WebSocket Support

- Real-time session updates
- Participant join/leave notifications
- Ping/pong for connection health
- Error notifications

### Daily.co Integration

- Automatic room creation with configurable privacy settings
- Meeting token generation with participant permissions
- Room presence monitoring
- Automatic cleanup when sessions end

## Configuration

### Environment Variables

```bash
# Daily.co Configuration
DAILY_API_KEY=your-daily-api-key-here
DAILY_DOMAIN=your-daily-domain-here

# Service Configuration
PORT=8084
NATS_URL=nats://localhost:4222
RUST_LOG=info
```

### Room Configuration Options

- **Privacy**: Public or Private rooms
- **Recording**: Enable/disable cloud recording
- **Transcription**: Enable/disable real-time transcription
- **Chat**: Enable/disable in-room chat
- **Screen Sharing**: Enable/disable screen sharing
- **Max Participants**: Set participant limits

### Participant Permissions

- **Owner Status**: Grant admin privileges
- **Video/Audio**: Control media permissions
- **Screen Sharing**: Allow screen sharing
- **Admin Rights**: Grant room administration

## API Usage

### Creating a Session

```bash
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "daily",
    "room_config": {
      "name": "my-room",
      "privacy": "Private",
      "max_participants": 10,
      "enable_recording": true,
      "enable_transcription": false,
      "enable_chat": true,
      "auto_start_recording": false,
      "auto_start_transcription": false
    },
    "participant_config": {
      "name": "John Doe",
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
```

### Response

```json
{
  "session_id": "uuid-here",
  "room_url": "https://your-domain.daily.co/room-name",
  "room_name": "room-name",
  "meeting_token": "eyJ...",
  "expires_at": "2024-01-01T12:00:00Z",
  "provider_metadata": {
    "daily_room_data": {...}
  }
}
```

### WebSocket Connection

```javascript
const ws = new WebSocket('ws://localhost:8084/ws');

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  switch (message.type) {
    case 'session_created':
      console.log('Session created:', message.session_id);
      break;
    case 'session_updated':
      console.log('Session updated:', message.session_id);
      break;
    case 'session_ended':
      console.log('Session ended:', message.session_id);
      break;
    case 'ping':
      ws.send(JSON.stringify({type: 'pong'}));
      break;
  }
};
```

## Docker Deployment

The service is containerized and included in the main `docker-compose.yml`:

```yaml
transport-service:
  build:
    context: .
    dockerfile: docker/Dockerfile.transport-service
  ports:
    - "8084:8084"
  environment:
    - DAILY_API_KEY=${DAILY_API_KEY}
    - DAILY_DOMAIN=${DAILY_DOMAIN}
    - NATS_URL=nats://nats:4222
```

## Integration with Pipecat Services

The Transport Service integrates with the existing Pipecat architecture:

1. **STT Service** → Processes audio input
2. **LLM Service** → Generates responses
3. **TTS Service** → Synthesizes speech
4. **Transport Service** → Delivers audio/video via WebRTC

All services communicate via NATS JetStream for high-performance event streaming.

## Development

### Running Locally

```bash
# Set environment variables
export DAILY_API_KEY="your-key"
export DAILY_DOMAIN="your-domain"

# Run the service
cargo run -p transport-service
```

### Testing

```bash
# Health check
curl http://localhost:8084/health

# Create a test session
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{"provider": "daily", "room_config": {...}, "participant_config": {...}}'
```

## Security Considerations

- API keys are passed via environment variables
- Meeting tokens have configurable expiration (default: 1 hour)
- Room privacy settings control access
- Participant permissions are enforced by Daily.co
- WebSocket connections support authentication (can be extended)

## Monitoring

The service exposes Prometheus metrics at `/metrics`:

- `active_sessions` - Number of active sessions
- `registered_connectors` - Number of available connectors
- `pending_messages` - NATS queue depth
- `delivered_messages` - Total processed messages

## Future Enhancements

- Support for additional WebRTC providers (Agora, Twilio, etc.)
- Enhanced WebSocket event system
- Session recording management
- Advanced participant management
- Integration with authentication systems
- Rate limiting and quotas
