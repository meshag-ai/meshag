use super::{ControlFrame, DataFrame, FrameWrapper, SystemFrame};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;

impl FrameWrapper {
    pub fn to_json_with_base64(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;

        if let FrameWrapper::Data(df) = self {
            match df {
                DataFrame::InputAudioRawFrame { audio_data, .. } => {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert(
                            "audio_data".to_string(),
                            Value::String(STANDARD.encode(audio_data)),
                        );
                    }
                }
                DataFrame::OutputAudioRawFrame { audio_data, .. } => {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert(
                            "audio_data".to_string(),
                            Value::String(STANDARD.encode(audio_data)),
                        );
                    }
                }
                DataFrame::ImageFrame { image_data, .. } => {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert(
                            "image_data".to_string(),
                            Value::String(STANDARD.encode(image_data)),
                        );
                    }
                }
                _ => {}
            }
        }

        Ok(serde_json::to_string(&value)?)
    }

    pub fn from_json_with_base64(json: &str) -> Result<Self> {
        let mut value: Value = serde_json::from_str(json)?;

        if let Some(obj) = value.as_object_mut() {
            if let Some(audio_b64) = obj.get("audio_data").and_then(|v| v.as_str()) {
                let bytes = STANDARD.decode(audio_b64)?;
                obj.insert("audio_data".to_string(), Value::Array(bytes.into_iter().map(|b| Value::Number(b.into())).collect()));
            }
            if let Some(image_b64) = obj.get("image_data").and_then(|v| v.as_str()) {
                let bytes = STANDARD.decode(image_b64)?;
                obj.insert("image_data".to_string(), Value::Array(bytes.into_iter().map(|b| Value::Number(b.into())).collect()));
            }
        }

        Ok(serde_json::from_value(value)?)
    }
}

impl DataFrame {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

impl SystemFrame {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

impl ControlFrame {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}
