use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_concurrent_sessions_isolation() {
    use meshag_shared::{DataFrame, FrameWrapper};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    let session_data: Arc<Mutex<HashMap<String, Vec<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut handles = vec![];

    for i in 0..10 {
        let session_id = format!("session-{}", i);
        let data = session_data.clone();

        let handle = tokio::spawn(async move {
            for j in 0..5 {
                let frame = FrameWrapper::Data(DataFrame::TextFrame {
                    session_id: session_id.clone(),
                    frame_id: Uuid::new_v4(),
                    timestamp: j,
                    text: format!("Message {} from session {}", j, i),
                    language: None,
                });

                {
                    let mut map = data.lock().unwrap();
                    map.entry(session_id.clone())
                        .or_insert_with(Vec::new)
                        .push(frame.session_id().to_string());
                }

                sleep(Duration::from_millis(10)).await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let final_data = session_data.lock().unwrap();
    assert_eq!(final_data.len(), 10);

    for i in 0..10 {
        let session_id = format!("session-{}", i);
        let messages = final_data.get(&session_id).unwrap();
        assert_eq!(messages.len(), 5);
    }
}

#[tokio::test]
async fn test_session_handler_lifecycle() {
    use dashmap::DashMap;
    use meshag_shared::{FrameWrapper, SystemFrame};
    use std::collections::HashMap;

    let sessions: Arc<DashMap<String, bool>> = Arc::new(DashMap::new());

    let session_id = "test-session".to_string();
    sessions.insert(session_id.clone(), true);

    let start_frame = FrameWrapper::System(SystemFrame::StartFrame {
        session_id: session_id.clone(),
        metadata: HashMap::new(),
    });

    assert_eq!(start_frame.session_id(), session_id);
    assert!(sessions.contains_key(&session_id));

    let end_frame = FrameWrapper::System(SystemFrame::EndFrame {
        session_id: session_id.clone(),
        reason: "test complete".to_string(),
    });

    assert_eq!(end_frame.session_id(), session_id);

    sessions.remove(&session_id);
    assert!(!sessions.contains_key(&session_id));
}

#[tokio::test]
async fn test_frame_filtering_by_type() {
    use meshag_shared::{DataFrame, FrameWrapper};
    use uuid::Uuid;

    fn can_process_transcription(frame: &FrameWrapper) -> bool {
        matches!(frame, FrameWrapper::Data(DataFrame::TranscriptionFrame { .. }))
    }

    let transcription = FrameWrapper::Data(DataFrame::TranscriptionFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "hello".to_string(),
        language: "en".to_string(),
        confidence: 0.95,
        is_final: true,
    });

    let text_frame = FrameWrapper::Data(DataFrame::TextFrame {
        session_id: "test".to_string(),
        frame_id: Uuid::new_v4(),
        timestamp: 0,
        text: "hello".to_string(),
        language: None,
    });

    assert!(can_process_transcription(&transcription));
    assert!(!can_process_transcription(&text_frame));
}

#[tokio::test]
async fn test_concurrent_frame_processing() {
    use meshag_shared::{DataFrame, FrameWrapper, LocalProcessor};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let processor = LocalProcessor::new("counter".to_string(), move |frame| {
        let c = counter_clone.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(vec![frame])
        })
    });

    let mut handles = vec![];

    for i in 0..50 {
        let proc = Arc::new(processor.clone());
        let handle = tokio::spawn(async move {
            let frame = FrameWrapper::Data(DataFrame::TextFrame {
                session_id: format!("session-{}", i % 10),
                frame_id: Uuid::new_v4(),
                timestamp: 0,
                text: format!("Message {}", i),
                language: None,
            });

            proc.process(frame).await
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 50);
}
