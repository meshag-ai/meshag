# Twilio Connector Integration

The Twilio connector has been successfully integrated into the MeshAG transport service, providing voice call capabilities alongside the existing Daily.co WebRTC support.

## Features

- **Voice Call Management**: Create, manage, and terminate Twilio voice calls
- **Call Status Tracking**: Monitor call status (queued, ringing, in-progress, completed)
- **Webhook Integration**: Receive call events via webhooks
- **Recording Support**: Automatic call recording with status callbacks
- **Health Monitoring**: Built-in health checks for Twilio API connectivity

## Configuration

### Environment Variables

Add the following environment variables to enable Twilio support:

```bash
# Twilio Credentials (Required)
TWILIO_ACCOUNT_SID=your_account_sid
TWILIO_AUTH_TOKEN=your_auth_token
TWILIO_PHONE_NUMBER=+1234567890

# Webhook URL (Optional but recommended)
TWILIO_WEBHOOK_URL=https://your-domain.com/webhooks/twilio
```

### Docker Compose

The Twilio environment variables are already configured in `docker/docker-compose.yml`:

```yaml
environment:
  # Twilio Configuration (Optional)
  - TWILIO_ACCOUNT_SID=${TWILIO_ACCOUNT_SID:-}
  - TWILIO_AUTH_TOKEN=${TWILIO_AUTH_TOKEN:-}
  - TWILIO_PHONE_NUMBER=${TWILIO_PHONE_NUMBER:-}
  - TWILIO_WEBHOOK_URL=${TWILIO_WEBHOOK_URL:-}
```

## Usage

### Creating a Twilio Session

The Twilio connector is automatically registered when valid credentials are provided. You can create sessions using either provider:

```bash
# Create a Daily.co session (existing functionality)
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "daily",
    "room_config": {
      "privacy": "public",
      "max_participants": 10
    },
    "participant_config": {
      "name": "user",
      "is_owner": true,
      "permissions": {
        "can_admin": true,
        "can_send_video": true,
        "can_send_audio": true,
        "can_send_screen_video": false,
        "can_send_screen_audio": false
      }
    }
  }'

# Create a Twilio session (new functionality)
curl -X POST http://localhost:8084/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "twilio",
    "room_config": {
      "privacy": "public",
      "max_participants": 2
    },
    "participant_config": {
      "name": "+1234567890",
      "is_owner": true,
      "permissions": {
        "can_admin": true,
        "can_send_video": false,
        "can_send_audio": true,
        "can_send_screen_video": false,
        "can_send_screen_audio": false
      }
    }
  }'
```

### Session with Configuration

You can also create sessions with AI agent configuration:

```bash
curl -X POST http://localhost:8084/sessions/with-config \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "twilio",
    "room_config": {
      "privacy": "public",
      "max_participants": 2
    },
    "participant_config": {
      "name": "+1234567890",
      "is_owner": true,
      "permissions": {
        "can_admin": true,
        "can_send_video": false,
        "can_send_audio": true,
        "can_send_screen_video": false,
        "can_send_screen_audio": false
      }
    },
    "config": {
      "system_prompt": "You are a helpful AI assistant for phone calls.",
      "pipeline": {
        "steps": [
          {
            "service": "stt",
            "connector": "openai",
            "config": {}
          },
          {
            "service": "llm",
            "connector": "openai",
            "config": {}
          },
          {
            "service": "tts",
            "connector": "elevenlabs",
            "config": {}
          }
        ]
      },
      "connectors": {
        "stt": {
          "provider": "openai",
          "config": {}
        },
        "llm": {
          "provider": "openai",
          "config": {}
        },
        "tts": {
          "provider": "elevenlabs",
          "config": {}
        }
      }
    }
  }'
```

## Twilio-Specific Features

### Call Management

- **Automatic Call Creation**: Sessions automatically create Twilio calls
- **Call Status Tracking**: Monitor call status through the session info endpoint
- **Call Termination**: End sessions to hang up calls

### Webhook Integration

When a webhook URL is configured, Twilio will send events to your endpoint:

- **Call Status Events**: initiated, ringing, answered, completed
- **Recording Events**: Recording start/stop notifications
- **Error Events**: Failed call attempts

### Health Monitoring

The connector includes health checks that verify Twilio API connectivity:

```bash
curl http://localhost:8084/health
```

## Implementation Details

### Connector Registration

The Twilio connector is automatically registered when valid credentials are provided:

```rust
// Register Twilio connector if credentials are provided
if !config.twilio_account_sid.is_empty()
    && !config.twilio_auth_token.is_empty()
    && !config.twilio_phone_number.is_empty() {
    let twilio_config = TwilioConfig::new(
        config.twilio_account_sid.clone(),
        config.twilio_auth_token.clone(),
        config.twilio_phone_number.clone(),
    ).with_webhook_url(config.twilio_webhook_url.clone());

    let twilio_connector = twilio_transport_connector(twilio_config);
    connectors.insert("twilio".to_string(), twilio_connector);
    tracing::info!("Twilio connector registered successfully");
}
```

### Session Provider Mapping

The transport service tracks which provider is used for each session:

```rust
// Store the provider mapping for this session
self.session_providers.insert(session_id, request.provider.clone());
```

### Provider Selection

You can query sessions by provider:

```bash
# Get all Twilio sessions
curl http://localhost:8084/sessions?provider=twilio

# Get session statistics
curl http://localhost:8084/metrics
```

## Error Handling

The connector gracefully handles various error scenarios:

- **Missing Credentials**: Connector is not registered if credentials are missing
- **API Errors**: Proper error messages for Twilio API failures
- **Network Issues**: Timeout and connection error handling
- **Invalid Phone Numbers**: Validation of phone number formats

## Monitoring

The transport service provides comprehensive monitoring for Twilio sessions:

- Session counts by provider
- Active vs completed sessions
- Provider-specific metrics
- Health check status

## Next Steps

The Twilio connector is now fully integrated and ready for use. You can:

1. Set up your Twilio credentials
2. Configure webhook endpoints
3. Start creating voice call sessions
4. Monitor call status and metrics
5. Integrate with your AI agent pipeline

For more advanced features, consider implementing:
- Call recording management
- Advanced call routing
- Multi-party conference calls
- Call analytics and reporting
