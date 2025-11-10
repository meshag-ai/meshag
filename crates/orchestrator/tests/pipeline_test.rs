use anyhow::Result;
use futures::future::BoxFuture;
use meshag_orchestrator::{Pipeline, PipelineBuilder};
use meshag_shared::{DataFrame, FrameWrapper, LocalProcessor, Processor, SystemFrame};
use uuid::Uuid;

#[tokio::test]
async fn test_pipeline_creation() {
    let pipeline = Pipeline::new("test-pipeline".to_string());
    assert_eq!(pipeline.name, "test-pipeline");
    assert_eq!(pipeline.processors.len(), 0);
}

#[tokio::test]
async fn test_pipeline_builder_with_local_fn() {
    let pipeline = PipelineBuilder::new("test".to_string())
        .add_local_fn("uppercase", |frame: FrameWrapper| {
            Box::pin(async move {
                if let FrameWrapper::Data(DataFrame::TextFrame {
                    text, session_id, ..
                }) = frame
                {
                    let uppercased = text.to_uppercase();
                    Ok(vec![FrameWrapper::Data(DataFrame::TextFrame {
                        session_id,
                        frame_id: Uuid::new_v4(),
                        timestamp: 0,
                        text: uppercased,
                        language: None,
                    })])
                } else {
                    Ok(vec![frame])
                }
            }) as BoxFuture<'static, Result<Vec<FrameWrapper>>>
        })
        .build();

    assert_eq!(pipeline.processors.len(), 1);
    assert_eq!(pipeline.name, "test");
}

#[tokio::test]
async fn test_pipeline_priority_queue_ordering() {
    let pipeline = Pipeline::new("test".to_string());

    let data_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 1,
        text: "data".to_string(),
        language: None,
    });

    let system_frame = FrameWrapper::System(SystemFrame::StartInterruptionFrame {
        session_id: "test".to_string(),
        source: "user".to_string(),
    });

    pipeline.push_frame(data_frame).await.unwrap();
    pipeline.push_frame(system_frame.clone()).await.unwrap();

    let first = pipeline.pop_frame().await.unwrap();
    assert_eq!(first.priority, 0);

    match first.frame {
        FrameWrapper::System(SystemFrame::StartInterruptionFrame { .. }) => {}
        _ => panic!("Expected system frame to be popped first"),
    }
}

#[tokio::test]
async fn test_pipeline_multiple_frames() {
    let pipeline = Pipeline::new("test".to_string());

    for i in 0..10 {
        let frame = FrameWrapper::Data(DataFrame::TextFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: i,
            text: format!("Message {}", i),
            language: None,
        });
        pipeline.push_frame(frame).await.unwrap();
    }

    for _ in 0..10 {
        let popped = pipeline.pop_frame().await;
        assert!(popped.is_some());
    }

    let empty = pipeline.pop_frame().await;
    assert!(empty.is_none());
}

#[tokio::test]
async fn test_local_processor_execution() {
    let processor = LocalProcessor::new("test-processor".to_string(), |frame: FrameWrapper| {
        Box::pin(async move {
            if let FrameWrapper::Data(DataFrame::TextFrame {
                text, session_id, ..
            }) = frame
            {
                Ok(vec![FrameWrapper::Data(DataFrame::TextFrame {
                    session_id,
                    frame_id: Uuid::new_v4(),
                    timestamp: 0,
                    text: format!("Processed: {}", text),
                    language: None,
                })])
            } else {
                Ok(vec![frame])
            }
        })
    });

    let input = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "hello".to_string(),
        language: None,
    });

    let output = processor.process(input).await.unwrap();
    assert_eq!(output.len(), 1);

    if let FrameWrapper::Data(DataFrame::TextFrame { text, .. }) = &output[0] {
        assert_eq!(text, "Processed: hello");
    } else {
        panic!("Expected TextFrame");
    }
}

#[tokio::test]
async fn test_processor_chain() {
    let pipeline = PipelineBuilder::new("chain-test".to_string())
        .add_local_fn("add-prefix", |frame: FrameWrapper| {
            Box::pin(async move {
                if let FrameWrapper::Data(DataFrame::TextFrame {
                    text, session_id, ..
                }) = frame
                {
                    Ok(vec![FrameWrapper::Data(DataFrame::TextFrame {
                        session_id,
                        frame_id: Uuid::new_v4(),
                        timestamp: 0,
                        text: format!("PREFIX: {}", text),
                        language: None,
                    })])
                } else {
                    Ok(vec![frame])
                }
            }) as BoxFuture<'static, Result<Vec<FrameWrapper>>>
        })
        .add_local_fn("add-suffix", |frame: FrameWrapper| {
            Box::pin(async move {
                if let FrameWrapper::Data(DataFrame::TextFrame {
                    text, session_id, ..
                }) = frame
                {
                    Ok(vec![FrameWrapper::Data(DataFrame::TextFrame {
                        session_id,
                        frame_id: Uuid::new_v4(),
                        timestamp: 0,
                        text: format!("{} :SUFFIX", text),
                        language: None,
                    })])
                } else {
                    Ok(vec![frame])
                }
            }) as BoxFuture<'static, Result<Vec<FrameWrapper>>>
        })
        .build();

    assert_eq!(pipeline.processors.len(), 2);
}

#[tokio::test]
async fn test_processor_can_drop_frames() {
    let processor = LocalProcessor::new("filter".to_string(), |frame: FrameWrapper| {
        Box::pin(async move {
            if let FrameWrapper::Data(DataFrame::TextFrame { ref text, .. }) = frame {
                if text.contains("spam") {
                    return Ok(vec![]);
                }
            }
            Ok(vec![frame])
        })
    });

    let spam_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "This is spam".to_string(),
        language: None,
    });

    let result = processor.process(spam_frame).await.unwrap();
    assert_eq!(result.len(), 0);

    let good_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "Good message".to_string(),
        language: None,
    });

    let result = processor.process(good_frame).await.unwrap();
    assert_eq!(result.len(), 1);
}

#[tokio::test]
async fn test_processor_can_emit_multiple_frames() {
    let processor = LocalProcessor::new("splitter".to_string(), |frame: FrameWrapper| {
        Box::pin(async move {
            if let FrameWrapper::Data(DataFrame::TextFrame {
                text, session_id, ..
            }) = frame
            {
                let words: Vec<&str> = text.split_whitespace().collect();
                let frames: Vec<FrameWrapper> = words
                    .into_iter()
                    .map(|word| {
                        FrameWrapper::Data(DataFrame::TextFrame {
                            session_id: session_id.clone(),
                            frame_id: Uuid::new_v4(),
                            timestamp: 0,
                            text: word.to_string(),
                            language: None,
                        })
                    })
                    .collect();
                Ok(frames)
            } else {
                Ok(vec![frame])
            }
        })
    });

    let input = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "one two three".to_string(),
        language: None,
    });

    let output = processor.process(input).await.unwrap();
    assert_eq!(output.len(), 3);
}
