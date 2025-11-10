use meshag_shared::{AudioFormat, DataFrame, FrameWrapper, ProcessingEvent, SystemFrame};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn test_text_frame_to_event() {
    let frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test-session".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "test message".to_string(),
        language: Some("en".to_string()),
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.session_id, "test-session");
    assert_eq!(event.event_type, "text");
    assert_eq!(
        event.payload.get("text").and_then(|v| v.as_str()),
        Some("test message")
    );
}

#[test]
fn test_transcription_frame_to_event() {
    let frame = FrameWrapper::Data(DataFrame::TranscriptionFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "transcribed text".to_string(),
        language: "en".to_string(),
        confidence: 0.95,
        is_final: true,
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.event_type, "transcription_output");
    assert_eq!(
        event.payload["transcription"]["text"].as_str(),
        Some("transcribed text")
    );
    assert_eq!(event.payload["transcription"]["confidence"].as_f64(), Some(0.95));
}

#[test]
fn test_llm_frame_to_event() {
    let frame = FrameWrapper::Data(DataFrame::LLMTextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "AI response".to_string(),
        model: "gpt-4".to_string(),
        is_complete: true,
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.event_type, "llm_response_complete");
    assert_eq!(event.payload.get("text").and_then(|v| v.as_str()), Some("AI response"));
    assert_eq!(event.payload.get("model").and_then(|v| v.as_str()), Some("gpt-4"));
}

#[test]
fn test_audio_frame_to_event() {
    let audio_data = vec![1, 2, 3, 4, 5];
    let frame = FrameWrapper::Data(DataFrame::InputAudioRawFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        audio_data: audio_data.clone(),
        sample_rate: 16000,
        num_channels: 1,
        format: AudioFormat::PCM16,
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.event_type, "audio_input");
    assert!(event.payload.get("audio_data").is_some());
    assert_eq!(event.payload.get("sample_rate").and_then(|v| v.as_u64()), Some(16000));
}

#[test]
fn test_system_frame_to_event() {
    let frame = FrameWrapper::System(SystemFrame::StartFrame {
        session_id: "test".to_string(),
        metadata: HashMap::new(),
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.event_type, "session_start");
    assert_eq!(event.session_id, "test");
}

#[test]
fn test_interrupt_frame_to_event() {
    let frame = FrameWrapper::System(SystemFrame::StartInterruptionFrame {
        session_id: "test".to_string(),
        source: "user".to_string(),
    });

    let event = frame.to_processing_event().unwrap();
    assert_eq!(event.event_type, "interrupt_start");
    assert_eq!(event.payload.get("source").and_then(|v| v.as_str()), Some("user"));
}

#[test]
fn test_event_to_start_frame() {
    let event = ProcessingEvent {
        session_id: "test-session".to_string(),
        conversation_id: Uuid::new_v4(),
        correlation_id: Uuid::new_v4(),
        event_type: "session_start".to_string(),
        payload: serde_json::json!({"key": "value"}),
        timestamp_ms: 123456789,
    };

    let frame = FrameWrapper::from_processing_event(event).unwrap();
    assert_eq!(frame.session_id(), "test-session");

    if let FrameWrapper::System(SystemFrame::StartFrame { metadata, .. }) = frame {
        assert!(metadata.contains_key("key"));
    } else {
        panic!("Expected StartFrame");
    }
}

#[test]
fn test_event_to_transcription_frame() {
    let event = ProcessingEvent {
        session_id: "test".to_string(),
        conversation_id: Uuid::new_v4(),
        correlation_id: Uuid::new_v4(),
        event_type: "transcription_output".to_string(),
        payload: serde_json::json!({
            "transcription": {
                "text": "hello world",
                "language": "en",
                "confidence": 0.95,
                "is_final": true
            }
        }),
        timestamp_ms: 123456789,
    };

    let frame = FrameWrapper::from_processing_event(event).unwrap();

    if let FrameWrapper::Data(DataFrame::TranscriptionFrame { text, confidence, is_final, .. }) = frame {
        assert_eq!(text, "hello world");
        assert_eq!(confidence, 0.95);
        assert!(is_final);
    } else {
        panic!("Expected TranscriptionFrame");
    }
}

#[test]
fn test_event_to_llm_frame() {
    let event = ProcessingEvent {
        session_id: "test".to_string(),
        conversation_id: Uuid::new_v4(),
        correlation_id: Uuid::new_v4(),
        event_type: "llm_response_complete".to_string(),
        payload: serde_json::json!({
            "text": "AI generated response",
            "model": "gpt-4"
        }),
        timestamp_ms: 123456789,
    };

    let frame = FrameWrapper::from_processing_event(event).unwrap();

    if let FrameWrapper::Data(DataFrame::LLMTextFrame { text, model, .. }) = frame {
        assert_eq!(text, "AI generated response");
        assert_eq!(model, "gpt-4");
    } else {
        panic!("Expected LLMTextFrame");
    }
}

#[test]
fn test_roundtrip_conversion() {
    let original_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "roundtrip test".to_string(),
        language: Some("en".to_string()),
    });

    let event = original_frame.to_processing_event().unwrap();
    let converted_frame = FrameWrapper::from_processing_event(event).unwrap();

    assert_eq!(original_frame.session_id(), converted_frame.session_id());

    if let FrameWrapper::Data(DataFrame::TextFrame { text, .. }) = converted_frame {
        assert_eq!(text, "roundtrip test");
    } else {
        panic!("Expected TextFrame");
    }
}

#[test]
fn test_unknown_event_type() {
    let event = ProcessingEvent {
        session_id: "test".to_string(),
        conversation_id: Uuid::new_v4(),
        correlation_id: Uuid::new_v4(),
        event_type: "unknown_event_type".to_string(),
        payload: serde_json::json!({}),
        timestamp_ms: 123456789,
    };

    let result = FrameWrapper::from_processing_event(event);
    assert!(result.is_err());
}
