use meshag_shared::{AudioFormat, ControlFrame, DataFrame, FrameCategory, FrameWrapper, ImageFormat, SystemFrame};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn test_text_frame_serialization() {
    let frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test-session".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "Hello, world!".to_string(),
        language: Some("en".to_string()),
    });

    let json = frame.to_json().unwrap();
    let deserialized = FrameWrapper::from_json(&json).unwrap();

    assert_eq!(frame.session_id(), deserialized.session_id());
}

#[test]
fn test_audio_frame_json() {
    let audio_data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let frame = FrameWrapper::Data(DataFrame::InputAudioRawFrame {
        session_id: "test-session".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        audio_data: audio_data.clone(),
        sample_rate: 16000,
        num_channels: 1,
        format: AudioFormat::PCM16,
    });

    let json = frame.to_json().unwrap();
    let deserialized = FrameWrapper::from_json(&json).unwrap();

    assert_eq!(frame.session_id(), deserialized.session_id());

    if let FrameWrapper::Data(DataFrame::InputAudioRawFrame { audio_data: data, .. }) = deserialized {
        assert_eq!(data, audio_data);
    } else {
        panic!("Expected InputAudioRawFrame");
    }
}

#[test]
fn test_transcription_frame() {
    let frame = FrameWrapper::Data(DataFrame::TranscriptionFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "Hello world".to_string(),
        language: "en".to_string(),
        confidence: 0.95,
        is_final: true,
    });

    let json = frame.to_json().unwrap();
    let deserialized = FrameWrapper::from_json(&json).unwrap();

    if let FrameWrapper::Data(DataFrame::TranscriptionFrame { text, confidence, is_final, .. }) = deserialized {
        assert_eq!(text, "Hello world");
        assert_eq!(confidence, 0.95);
        assert!(is_final);
    } else {
        panic!("Expected TranscriptionFrame");
    }
}

#[test]
fn test_llm_text_frame() {
    let frame = FrameWrapper::Data(DataFrame::LLMTextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "AI response text".to_string(),
        model: "gpt-4".to_string(),
        is_complete: true,
    });

    let json = frame.to_json().unwrap();
    let deserialized = FrameWrapper::from_json(&json).unwrap();

    if let FrameWrapper::Data(DataFrame::LLMTextFrame { text, model, is_complete, .. }) = deserialized {
        assert_eq!(text, "AI response text");
        assert_eq!(model, "gpt-4");
        assert!(is_complete);
    } else {
        panic!("Expected LLMTextFrame");
    }
}

#[test]
fn test_image_frame() {
    let image_data = vec![255, 216, 255, 224];
    let frame = FrameWrapper::Data(DataFrame::ImageFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        image_data: image_data.clone(),
        format: ImageFormat::JPEG,
        width: 640,
        height: 480,
    });

    let json = frame.to_json().unwrap();
    let deserialized = FrameWrapper::from_json(&json).unwrap();

    if let FrameWrapper::Data(DataFrame::ImageFrame { image_data: data, width, height, .. }) = deserialized {
        assert_eq!(data, image_data);
        assert_eq!(width, 640);
        assert_eq!(height, 480);
    } else {
        panic!("Expected ImageFrame");
    }
}

#[test]
fn test_frame_priority_ordering() {
    let system_frame = FrameWrapper::System(SystemFrame::StartFrame {
        session_id: "test".to_string(),
        metadata: HashMap::new(),
    });

    let control_frame = FrameWrapper::Control(ControlFrame::TTSStartedFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
    });

    let data_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 123456789,
        text: "test".to_string(),
        language: None,
    });

    assert_eq!(system_frame.frame_category(), FrameCategory::System);
    assert_eq!(control_frame.frame_category(), FrameCategory::Control);
    assert_eq!(data_frame.frame_category(), FrameCategory::Data);

    assert!(system_frame.frame_category() < control_frame.frame_category());
    assert!(control_frame.frame_category() < data_frame.frame_category());
}

#[test]
fn test_all_system_frames() {
    let frames = vec![
        FrameWrapper::System(SystemFrame::StartFrame {
            session_id: "test".to_string(),
            metadata: HashMap::new(),
        }),
        FrameWrapper::System(SystemFrame::EndFrame {
            session_id: "test".to_string(),
            reason: "completed".to_string(),
        }),
        FrameWrapper::System(SystemFrame::StartInterruptionFrame {
            session_id: "test".to_string(),
            source: "user".to_string(),
        }),
        FrameWrapper::System(SystemFrame::StopInterruptionFrame {
            session_id: "test".to_string(),
        }),
        FrameWrapper::System(SystemFrame::ErrorFrame {
            session_id: "test".to_string(),
            error: "test error".to_string(),
            recoverable: true,
        }),
        FrameWrapper::System(SystemFrame::UserStartedSpeakingFrame {
            session_id: "test".to_string(),
        }),
        FrameWrapper::System(SystemFrame::UserStoppedSpeakingFrame {
            session_id: "test".to_string(),
        }),
        FrameWrapper::System(SystemFrame::CancelFrame {
            session_id: "test".to_string(),
        }),
    ];

    for frame in frames {
        assert_eq!(frame.session_id(), "test");
        assert_eq!(frame.frame_category(), FrameCategory::System);

        let json = frame.to_json().unwrap();
        let deserialized = FrameWrapper::from_json(&json).unwrap();
        assert_eq!(deserialized.session_id(), "test");
    }
}

#[test]
fn test_all_control_frames() {
    let frames = vec![
        FrameWrapper::Control(ControlFrame::TTSStartedFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
        FrameWrapper::Control(ControlFrame::TTSStoppedFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
        FrameWrapper::Control(ControlFrame::LLMFullResponseStartFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
        FrameWrapper::Control(ControlFrame::LLMFullResponseEndFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
        FrameWrapper::Control(ControlFrame::BotStartedSpeakingFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
        FrameWrapper::Control(ControlFrame::BotStoppedSpeakingFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: 123456789,
        }),
    ];

    for frame in frames {
        assert_eq!(frame.session_id(), "test");
        assert_eq!(frame.frame_category(), FrameCategory::Control);

        let json = frame.to_json().unwrap();
        let deserialized = FrameWrapper::from_json(&json).unwrap();
        assert_eq!(deserialized.session_id(), "test");
    }
}

#[test]
fn test_invalid_json_deserialization() {
    let result = FrameWrapper::from_json("invalid json");
    assert!(result.is_err());
}
