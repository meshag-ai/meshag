# WebSocket Session Management in Transport Connectors

The transport connectors now manage their own WebSocket sessions, and the transport service calls the connector's methods to read and write to WebSocket connections.

## Twilio Connector WebSocket Management

The Twilio connector now includes comprehensive WebSocket session management:

### Features

- **Automatic WebSocket Connection**: WebSocket connections are automatically established when sessions are created
- **Connection Management**: Connectors maintain their own WebSocket connection pools
- **Read/Write Operations**: Transport service can read and write messages through connector methods
- **Connection Status**: Check if WebSocket connections are active
- **Automatic Cleanup**: WebSocket connections are automatically closed when sessions end

### Twilio Connector Methods

#### Internal Methods (used by the connector itself)
- `connect_websocket(session_id)` - Establishes WebSocket connection
- `disconnect_websocket(session_id)` - Closes WebSocket connection
- `read_websocket(session_id)` - Internal read method
- `write_websocket(session_id, message)` - Internal write method

#### Public Methods (used by transport service)
- `read_message(session_id)` - Read message from WebSocket
- `write_message(session_id, message)` - Write message to WebSocket
- `is_websocket_connected(session_id)` - Check connection status

### Transport Service Integration

The transport service now provides methods to interact with connector WebSocket sessions:

```rust
// Read a message from WebSocket via connector
let message = transport_service.read_websocket_message(session_id).await?;

// Write a message to WebSocket via connector
transport_service.write_websocket_message(session_id, message).await?;

// Check if WebSocket is connected
let is_connected = transport_service.is_websocket_connected(session_id).await?;
```

### Usage Example

```rust
// Create a Twilio session (automatically establishes WebSocket)
let response = transport_service.create_session(request).await?;
let session_id = response.session_id;

// Check if WebSocket is connected
if transport_service.is_websocket_connected(&session_id).await? {
    // Read messages from WebSocket
    while let Some(message) = transport_service.read_websocket_message(&session_id).await? {
        match message {
            Message::Binary(audio_data) => {
                // Process audio data
                println!("Received {} bytes of audio", audio_data.len());
            }
            Message::Text(text) => {
                // Process text message
                println!("Received text: {}", text);
            }
            Message::Close(_) => {
                println!("WebSocket connection closed");
                break;
            }
            _ => {}
        }
    }
}

// Send audio data to WebSocket
let audio_message = Message::Binary(audio_data);
transport_service.write_websocket_message(&session_id, audio_message).await?;

// End session (automatically closes WebSocket)
transport_service.end_session(&session_id).await?;
```

### Architecture Benefits

1. **Separation of Concerns**: Connectors handle their own WebSocket management
2. **Provider-Specific Logic**: Each connector can implement WebSocket handling according to their API
3. **Simplified Transport Service**: Transport service focuses on orchestration, not WebSocket details
4. **Extensibility**: Easy to add new providers with different WebSocket implementations
5. **Error Isolation**: WebSocket errors are contained within the connector

### Implementation Details

#### Twilio Connector WebSocket Management

```rust
pub struct TwilioTransportConnector {
    config: TwilioConfig,
    client: reqwest::Client,
    websocket_connections: Arc<RwLock<HashMap<String, Arc<RwLock<Option<WebSocketStream<...>>>>>>>,
}
```

- **Connection Pool**: Maintains a HashMap of WebSocket connections by session ID
- **Thread Safety**: Uses `Arc<RwLock<...>>` for concurrent access
- **Automatic Management**: Connections are created/closed with session lifecycle

#### Transport Service WebSocket Interface

```rust
impl TransportServiceState {
    pub async fn read_websocket_message(&self, session_id: &str) -> Result<Option<Message>> {
        // Get provider for session
        // Cast connector to appropriate type
        // Call connector's read method
    }

    pub async fn write_websocket_message(&self, session_id: &str, message: Message) -> Result<()> {
        // Get provider for session
        // Cast connector to appropriate type
        // Call connector's write method
    }

    pub async fn is_websocket_connected(&self, session_id: &str) -> Result<bool> {
        // Get provider for session
        // Cast connector to appropriate type
        // Call connector's connection check method
    }
}
```

### Future Enhancements

1. **Daily.co Connector**: Implement similar WebSocket management for Daily.co
2. **Connection Pooling**: Implement connection pooling for better performance
3. **Reconnection Logic**: Add automatic reconnection for dropped connections
4. **Message Queuing**: Add message queuing for offline scenarios
5. **Metrics**: Add WebSocket connection metrics and monitoring

### Error Handling

The implementation includes comprehensive error handling:

- **Connection Errors**: Graceful handling of WebSocket connection failures
- **Provider Errors**: Proper error propagation from connectors
- **Session Errors**: Clear error messages for invalid sessions
- **Type Casting Errors**: Safe casting with proper error messages

This architecture provides a clean separation between transport service orchestration and provider-specific WebSocket management, making the system more maintainable and extensible.
