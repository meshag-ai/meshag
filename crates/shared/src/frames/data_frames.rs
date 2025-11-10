use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    PCM16,
    PCM24,
    PCMU,
    PCMA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    PNG,
    JPEG,
    WEBP,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DataFrame {
    InputAudioRawFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        #[serde(with = "serde_bytes")]
        audio_data: Vec<u8>,
        sample_rate: u32,
        num_channels: u16,
        format: AudioFormat,
    },
    OutputAudioRawFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        #[serde(with = "serde_bytes")]
        audio_data: Vec<u8>,
        sample_rate: u32,
        num_channels: u16,
        format: AudioFormat,
    },
    TextFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        text: String,
        language: Option<String>,
    },
    TranscriptionFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        text: String,
        language: String,
        confidence: f32,
        is_final: bool,
    },
    LLMTextFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        text: String,
        model: String,
        is_complete: bool,
    },
    ImageFrame {
        session_id: String,
        frame_id: Uuid,
        timestamp: u64,
        #[serde(with = "serde_bytes")]
        image_data: Vec<u8>,
        format: ImageFormat,
        width: u32,
        height: u32,
    },
}

impl DataFrame {
    pub fn session_id(&self) -> &str {
        match self {
            DataFrame::InputAudioRawFrame { session_id, .. } => session_id,
            DataFrame::OutputAudioRawFrame { session_id, .. } => session_id,
            DataFrame::TextFrame { session_id, .. } => session_id,
            DataFrame::TranscriptionFrame { session_id, .. } => session_id,
            DataFrame::LLMTextFrame { session_id, .. } => session_id,
            DataFrame::ImageFrame { session_id, .. } => session_id,
        }
    }

    pub fn frame_id(&self) -> Uuid {
        match self {
            DataFrame::InputAudioRawFrame { frame_id, .. } => *frame_id,
            DataFrame::OutputAudioRawFrame { frame_id, .. } => *frame_id,
            DataFrame::TextFrame { frame_id, .. } => *frame_id,
            DataFrame::TranscriptionFrame { frame_id, .. } => *frame_id,
            DataFrame::LLMTextFrame { frame_id, .. } => *frame_id,
            DataFrame::ImageFrame { frame_id, .. } => *frame_id,
        }
    }

    pub fn timestamp(&self) -> u64 {
        match self {
            DataFrame::InputAudioRawFrame { timestamp, .. } => *timestamp,
            DataFrame::OutputAudioRawFrame { timestamp, .. } => *timestamp,
            DataFrame::TextFrame { timestamp, .. } => *timestamp,
            DataFrame::TranscriptionFrame { timestamp, .. } => *timestamp,
            DataFrame::LLMTextFrame { timestamp, .. } => *timestamp,
            DataFrame::ImageFrame { timestamp, .. } => *timestamp,
        }
    }
}
