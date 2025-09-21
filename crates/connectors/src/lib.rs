pub mod llm;
pub mod providers;
pub mod stt;
pub mod transport;
pub mod tts;

// Re-export the main traits and types
pub use llm::{ChatMessage, LlmConnector, LlmRequest, LlmResponse, MessageRole, TokenUsage};
pub use stt::{AudioFormat, SttConnector, SttRequest, SttResponse};
pub use transport::{
    ParticipantConfig, ParticipantInfo, ParticipantPermissions, RoomConfig, RoomPrivacy,
    SessionInfo, SessionStatus, TransportConnector, TransportRequest, TransportResponse,
};
pub use tts::{TtsConnector, TtsRequest, TtsResponse, Voice};

// Re-export provider modules for easy access
pub use providers::{
    anthropic::{Anthropic, AnthropicConfig},
    azure::{Azure, AzureConfig},
    daily::{Daily, DailyConfig},
    deepgram::{Deepgram, DeepgramConfig},
    elevenlabs::{ElevenLabs, ElevenLabsConfig},
    openai::{OpenAI, OpenAIConfig},
};
