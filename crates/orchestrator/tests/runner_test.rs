use anyhow::Result;
use futures::future::BoxFuture;
use meshag_orchestrator::{Pipeline, PipelineBuilder, Runner, RunnerStatus};
use meshag_shared::{DataFrame, FrameWrapper, SystemFrame};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[tokio::test]
async fn test_runner_creation() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    let status = runner.get_status().await;
    assert_eq!(status, RunnerStatus::Idle);
}

#[tokio::test]
async fn test_runner_start_stop() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    runner.start().await.unwrap();
    assert_eq!(runner.get_status().await, RunnerStatus::Running);

    runner.stop().await.unwrap();
    assert_eq!(runner.get_status().await, RunnerStatus::Stopped);
}

#[tokio::test]
async fn test_runner_push_frame() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    let frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "test".to_string(),
        language: None,
    });

    runner.push_frame(frame).await.unwrap();

    let popped = runner.pipeline.pop_frame().await;
    assert!(popped.is_some());
}

#[tokio::test]
async fn test_runner_interrupt_handling() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    runner.start().await.unwrap();

    for i in 0..5 {
        let frame = FrameWrapper::Data(DataFrame::TextFrame {
            session_id: "test".to_string(),
            frame_id: Uuid::new_v4(),
            timestamp: i,
            text: format!("Message {}", i),
            language: None,
        });
        runner.push_frame(frame).await.unwrap();
    }

    let interrupt = FrameWrapper::System(SystemFrame::StartInterruptionFrame {
        session_id: "test".to_string(),
        source: "test".to_string(),
    });
    runner.push_frame(interrupt).await.unwrap();

    let interrupt_frame = runner.pipeline.pop_frame().await.unwrap();
    assert_eq!(interrupt_frame.priority, 0);

    runner.process_next_frame().await.unwrap();
}

#[tokio::test]
async fn test_runner_processes_system_frames_first() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    runner.start().await.unwrap();

    let data_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "data".to_string(),
        language: None,
    });

    let system_frame = FrameWrapper::System(SystemFrame::UserStartedSpeakingFrame {
        session_id: "test".to_string(),
    });

    runner.push_frame(data_frame).await.unwrap();
    runner.push_frame(system_frame).await.unwrap();

    let first = runner.pipeline.pop_frame().await.unwrap();
    assert_eq!(first.priority, 0);
}

#[tokio::test]
async fn test_runner_end_frame_stops_runner() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    runner.start().await.unwrap();
    assert_eq!(runner.get_status().await, RunnerStatus::Running);

    let end_frame = FrameWrapper::System(SystemFrame::EndFrame {
        session_id: "test".to_string(),
        reason: "test complete".to_string(),
    });

    runner.push_frame(end_frame).await.unwrap();
    runner.process_next_frame().await.unwrap();

    assert_eq!(runner.get_status().await, RunnerStatus::Stopped);
}

#[tokio::test]
async fn test_runner_metrics() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    let metrics = runner.get_metrics().await;
    assert_eq!(metrics.frames_processed, 0);
    assert_eq!(metrics.errors, 0);
}

#[tokio::test]
async fn test_runner_with_processor() {
    let processed = Arc::new(RwLock::new(Vec::new()));
    let processed_clone = processed.clone();

    let pipeline = PipelineBuilder::new("test".to_string())
        .add_local_fn("collector", move |frame: FrameWrapper| {
            let processed = processed_clone.clone();
            Box::pin(async move {
                if let FrameWrapper::Data(DataFrame::TextFrame { text, .. }) = &frame {
                    processed.write().await.push(text.clone());
                }
                Ok(vec![frame])
            }) as BoxFuture<'static, Result<Vec<FrameWrapper>>>
        })
        .build();

    let runner = Runner::new(pipeline);
    runner.start().await.unwrap();

    let frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "test message".to_string(),
        language: None,
    });

    runner.push_frame(frame).await.unwrap();

    let had_frame = runner.process_next_frame().await.unwrap();
    assert!(had_frame);

    let results = processed.read().await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], "test message");
}

#[tokio::test]
async fn test_runner_empty_queue_returns_false() {
    let pipeline = Pipeline::new("test".to_string());
    let runner = Runner::new(pipeline);

    runner.start().await.unwrap();

    let had_frame = runner.process_next_frame().await.unwrap();
    assert!(!had_frame);
}
