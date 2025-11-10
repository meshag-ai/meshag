use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

pub mod compat;
pub mod control_frames;
pub mod data_frames;
pub mod serialization;
pub mod system_frames;

pub use control_frames::ControlFrame;
pub use data_frames::{AudioFormat, DataFrame, ImageFormat};
pub use system_frames::SystemFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FrameCategory {
    System = 0,
    Control = 1,
    Data = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameType {
    StartFrame,
    EndFrame,
    StartInterruptionFrame,
    StopInterruptionFrame,
    ErrorFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
    CancelFrame,
    InputAudioRawFrame,
    OutputAudioRawFrame,
    TextFrame,
    TranscriptionFrame,
    LLMTextFrame,
    ImageFrame,
    TTSStartedFrame,
    TTSStoppedFrame,
    LLMFullResponseStartFrame,
    LLMFullResponseEndFrame,
    BotStartedSpeakingFrame,
    BotStoppedSpeakingFrame,
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait Frame: Send + Sync {
    fn frame_id(&self) -> Uuid;
    fn timestamp(&self) -> u64;
    fn session_id(&self) -> &str;
    fn frame_type(&self) -> FrameType;
    fn frame_category(&self) -> FrameCategory;
    fn priority(&self) -> u8 {
        self.frame_category() as u8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "frame_type")]
pub enum FrameWrapper {
    #[serde(rename = "StartFrame")]
    System(SystemFrame),
    #[serde(rename = "DataFrame")]
    Data(DataFrame),
    #[serde(rename = "ControlFrame")]
    Control(ControlFrame),
}

impl FrameWrapper {
    pub fn frame_category(&self) -> FrameCategory {
        match self {
            FrameWrapper::System(_) => FrameCategory::System,
            FrameWrapper::Data(_) => FrameCategory::Data,
            FrameWrapper::Control(_) => FrameCategory::Control,
        }
    }

    pub fn session_id(&self) -> &str {
        match self {
            FrameWrapper::System(f) => f.session_id(),
            FrameWrapper::Data(f) => f.session_id(),
            FrameWrapper::Control(f) => f.session_id(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }
}
