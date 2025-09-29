use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use meshag_connectors::providers::deepgram::DeepgramSttConnector;
use meshag_connectors::{AudioFormat, SttConnector};
use meshag_orchestrator::ServiceRouter;
use meshag_service_common::{HealthCheck, ServiceState};
use meshag_shared::MediaEventPayload;
use meshag_shared::{EventQueue, ProcessingEvent};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug)]
pub struct SessionState {
    pub session_id: String,
    pub is_established: bool,
    pub audio_format: Option<AudioFormat>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub buffered_audio: Vec<Vec<u8>>,
}

impl SessionState {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            is_established: false,
            audio_format: None,
            sample_rate: None,
            channels: None,
            buffered_audio: Vec::new(),
        }
    }
}

pub struct SttService {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn SttConnector>>>>,
    default_connector: Arc<RwLock<Option<String>>>,
    router: Arc<RwLock<Option<ServiceRouter>>>,
    session_states: Arc<RwLock<HashMap<String, Arc<Mutex<SessionState>>>>>,
}

impl SttService {
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            default_connector: Arc::new(RwLock::new(None)),
            router: Arc::new(RwLock::new(None)),
            session_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_connector(&mut self, name: &str, connector: Arc<dyn SttConnector>) {
        let mut connectors = self.connectors.write().await;
        connectors.insert(name.to_string(), connector);

        let mut default = self.default_connector.write().await;
        if default.is_none() {
            *default = Some(name.to_string());
        }
    }

    pub async fn get_connector(&self, name: Option<&str>) -> Option<Arc<dyn SttConnector>> {
        let connectors = self.connectors.read().await;

        let connector_name = match name {
            Some(n) => n.to_string(),
            None => {
                let default = self.default_connector.read().await;
                match default.as_ref() {
                    Some(name) => name.clone(),
                    None => return None,
                }
            }
        };

        let connector_name = connector_name.as_str();

        connectors.get(connector_name).map(|c| c.clone())
    }

    pub async fn start_processing(&self, queue: EventQueue) -> Result<()> {
        info!("STT service starting to consume from AUDIO_INPUT");

        let service = Arc::new(self.clone());
        let queue_clone = queue.clone();

        tokio::spawn(async move {
            let q_clone = queue_clone.clone();
            let stream_config = meshag_shared::StreamConfig::audio_input();

            if let Err(e) = queue_clone
                .consume_events(
                    stream_config,
                    "AUDIO_INPUT.session.*".to_string(),
                    move |event| {
                        let svc = Arc::clone(&service);
                        let q = q_clone.clone();
                        async move { handle_audio_event(svc, q, event).await }
                    },
                )
                .await
            {
                error!("Failed to consume audio input events: {}", e);
            }
        });

        // Start transcription polling task
        let service_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            poll_transcriptions(service_clone, queue).await;
        });

        Ok(())
    }

    pub async fn list_connectors(&self) -> Vec<String> {
        let connectors = self.connectors.read().await;
        connectors.keys().cloned().collect()
    }

    pub async fn health_check_connectors(&self) -> HashMap<String, bool> {
        let connectors = self.connectors.read().await;
        let mut results = HashMap::new();

        for (name, connector) in connectors.iter() {
            let health = connector.health_check().await.unwrap_or(false);
            results.insert(name.clone(), health);
        }

        results
    }

    pub async fn create_streaming_session(
        &self,
        session_id: String,
        sample_rate: u32,
        channels: u32,
        format: AudioFormat,
        language: Option<String>,
    ) -> Result<()> {
        let connectors = self.connectors.read().await;
        let default_name = self.default_connector.read().await;

        let connector_key = default_name.as_deref();
        let connector = match connector_key.and_then(|name| connectors.get(name)) {
            Some(connector) => connector,
            None => {
                error!("No STT connector found for session creation");
                return Err(anyhow::anyhow!("No STT connector available"));
            }
        };

        if let Some(deepgram) = connector.as_any().downcast_ref::<DeepgramSttConnector>() {
            deepgram
                .create_session(session_id, sample_rate, channels, format, language)
                .await?;
            info!("Created streaming session for Deepgram");
        } else {
            return Err(anyhow::anyhow!(
                "Connector does not support streaming sessions"
            ));
        }

        Ok(())
    }

    pub async fn send_audio_to_session(&self, session_id: &str, audio_data: Vec<u8>) -> Result<()> {
        let connectors = self.connectors.read().await;
        let default_name = self.default_connector.read().await;

        let connector_key = default_name.as_deref();
        let connector = match connector_key.and_then(|name| connectors.get(name)) {
            Some(connector) => connector,
            None => {
                error!("No STT connector found for audio sending");
                return Err(anyhow::anyhow!("No STT connector available"));
            }
        };

        if let Some(deepgram) = connector.as_any().downcast_ref::<DeepgramSttConnector>() {
            deepgram.send_audio(session_id, audio_data).await?;
        } else {
            return Err(anyhow::anyhow!(
                "Connector does not support streaming sessions"
            ));
        }

        Ok(())
    }

    pub async fn get_session_transcription(
        &self,
        session_id: &str,
    ) -> Result<Option<meshag_connectors::SttResponse>> {
        let connectors = self.connectors.read().await;
        let default_name = self.default_connector.read().await;

        let connector_key = default_name.as_deref();
        let connector = match connector_key.and_then(|name| connectors.get(name)) {
            Some(connector) => connector,
            None => {
                error!("No STT connector found for transcription retrieval");
                return Err(anyhow::anyhow!("No STT connector available"));
            }
        };

        if let Some(deepgram) = connector.as_any().downcast_ref::<DeepgramSttConnector>() {
            deepgram.get_transcription(session_id).await
        } else {
            Err(anyhow::anyhow!(
                "Connector does not support streaming sessions"
            ))
        }
    }

    pub async fn close_streaming_session(&self, session_id: &str) -> Result<()> {
        let connectors = self.connectors.read().await;
        let default_name = self.default_connector.read().await;

        let connector_key = default_name.as_deref();
        let connector = match connector_key.and_then(|name| connectors.get(name)) {
            Some(connector) => connector,
            None => {
                error!("No STT connector found for session closure");
                return Err(anyhow::anyhow!("No STT connector available"));
            }
        };

        if let Some(deepgram) = connector.as_any().downcast_ref::<DeepgramSttConnector>() {
            deepgram.close_session(session_id).await?;
            info!("Closed streaming session: {}", session_id);
        } else {
            return Err(anyhow::anyhow!(
                "Connector does not support streaming sessions"
            ));
        }

        Ok(())
    }

    async fn get_or_create_session_state(&self, session_id: String) -> Arc<Mutex<SessionState>> {
        let mut states = self.session_states.write().await;

        if let Some(state) = states.get(&session_id) {
            Arc::clone(state)
        } else {
            let state = Arc::new(Mutex::new(SessionState::new(session_id.clone())));
            states.insert(session_id, Arc::clone(&state));
            state
        }
    }

    async fn establish_streaming_session(
        &self,
        session_id: String,
        media_payload: &MediaEventPayload,
    ) -> Result<()> {
        let format: Result<AudioFormat, strum::ParseError> =
            AudioFormat::from_str(&media_payload.media_format.encoding);

        let audio_format = match format {
            Ok(format) => format,
            Err(e) => {
                error!(
                    "Unsupported audio format for session {}: {:?}",
                    session_id, e
                );
                return Err(anyhow::anyhow!("Unsupported audio format"));
            }
        };

        self.create_streaming_session(
            session_id.clone(),
            media_payload.media_format.sample_rate,
            media_payload.media_format.channels,
            audio_format.clone(),
            None,
        )
        .await?;

        let session_state = self.get_or_create_session_state(session_id.clone()).await;
        let mut state = session_state.lock().await;

        state.is_established = true;
        state.audio_format = Some(audio_format);
        state.sample_rate = Some(media_payload.media_format.sample_rate);
        state.channels = Some(media_payload.media_format.channels);

        for audio_data in state.buffered_audio.drain(..) {
            if let Err(e) = self.send_audio_to_session(&session_id, audio_data).await {
                error!(
                    "Failed to send buffered audio to session {}: {}",
                    session_id, e
                );
            }
        }

        info!(
            "Established streaming session and flushed {} buffered audio chunks for session: {}",
            state.buffered_audio.len(),
            session_id
        );

        Ok(())
    }

    async fn remove_session_state(&self, session_id: &str) {
        let mut states = self.session_states.write().await;
        states.remove(session_id);
    }
}

impl Clone for SttService {
    fn clone(&self) -> Self {
        Self {
            connectors: Arc::clone(&self.connectors),
            default_connector: Arc::clone(&self.default_connector),
            router: Arc::clone(&self.router),
            session_states: Arc::clone(&self.session_states),
        }
    }
}

async fn handle_audio_event(
    service: Arc<SttService>,
    queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    match event.event_type.as_str() {
        "media_input" => handle_audio_chunk(service, queue, event).await?,
        "session_start" => handle_session_start(service, event).await?,
        "session_end" => handle_session_end(service, event).await?,
        _ => error!("Unknown event type: {}", event.event_type),
    }
    Ok(())
}

async fn handle_audio_chunk(
    service: Arc<SttService>,
    _queue: EventQueue,
    event: ProcessingEvent,
) -> Result<()> {
    let media_payload: MediaEventPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| anyhow::anyhow!("Failed to parse MediaEventPayload: {}", e))?;

    let session_id = event.session_id.to_string();

    let audio_data = STANDARD
        .decode(&media_payload.payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64 audio data: {}", e))?;

    let session_state = service
        .get_or_create_session_state(session_id.clone())
        .await;
    let mut state = session_state.lock().await;

    if state.is_established {
        drop(state);
        if let Err(e) = service.send_audio_to_session(&session_id, audio_data).await {
            error!("Failed to send audio to streaming session: {}", e);
            return Err(e);
        }
    } else {
        info!(
            "Session {} not established yet, buffering audio chunk",
            session_id
        );
        state.buffered_audio.push(audio_data);

        if state.audio_format.is_none() {
            drop(state);

            if let Err(e) = service
                .establish_streaming_session(session_id.clone(), &media_payload)
                .await
            {
                error!(
                    "Failed to establish streaming session for {}: {}",
                    session_id, e
                );
            }
        }
    }

    Ok(())
}

async fn handle_session_start(service: Arc<SttService>, event: ProcessingEvent) -> Result<()> {
    let session_id = event.session_id.to_string();
    info!(session_id = %session_id, "STT session started");

    let session_state = service
        .get_or_create_session_state(session_id.clone())
        .await;
    let state = session_state.lock().await;

    if state.is_established {
        info!(
            "Session {} already established from early media events",
            session_id
        );
        return Ok(());
    }

    if let (Some(audio_format), Some(sample_rate), Some(channels)) = (
        state.audio_format.clone(),
        state.sample_rate,
        state.channels,
    ) {
        drop(state);

        if let Err(e) = service
            .create_streaming_session(
                session_id.clone(),
                sample_rate,
                channels,
                audio_format,
                None,
            )
            .await
        {
            error!(
                "Failed to create streaming session from buffered info: {}",
                e
            );
            return Err(e);
        }

        let mut state = session_state.lock().await;
        state.is_established = true;

        let buffered_count = state.buffered_audio.len();
        let buffered_audio: Vec<Vec<u8>> = state.buffered_audio.drain(..).collect();
        drop(state);

        for audio_data in buffered_audio {
            if let Err(e) = service.send_audio_to_session(&session_id, audio_data).await {
                error!(
                    "Failed to send buffered audio to session {}: {}",
                    session_id, e
                );
            }
        }

        info!(
            "Established session {} from session_start and flushed {} buffered audio chunks",
            session_id, buffered_count
        );
    } else {
        drop(state);

        if let Ok(media_payload) =
            serde_json::from_value::<MediaEventPayload>(event.payload.clone())
        {
            if let Err(e) = service
                .establish_streaming_session(session_id.clone(), &media_payload)
                .await
            {
                error!(
                    "Failed to establish streaming session from session_start: {}",
                    e
                );
                return Err(e);
            }
        } else {
            info!("Session {} started but no media format info available yet, waiting for media events", session_id);
        }
    }

    Ok(())
}

async fn handle_session_end(service: Arc<SttService>, event: ProcessingEvent) -> Result<()> {
    let session_id = event.session_id.to_string();
    info!(session_id = %session_id, "STT session ended");

    if let Err(e) = service.close_streaming_session(&session_id).await {
        error!("Failed to close streaming session: {}", e);
    }

    service.remove_session_state(&session_id).await;
    info!("Cleaned up session state for: {}", session_id);

    Ok(())
}

async fn poll_transcriptions(service: Arc<SttService>, queue: EventQueue) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

    loop {
        interval.tick().await;

        let connectors = service.connectors.read().await;
        let default_name = service.default_connector.read().await;

        if let Some(connector_name) = default_name.as_deref() {
            if let Some(connector) = connectors.get(connector_name) {
                if let Some(deepgram) = connector.as_any().downcast_ref::<DeepgramSttConnector>() {
                    let sessions = deepgram.sessions.read().await;

                    for (session_id, session_conn) in sessions.iter() {
                        let mut session = session_conn.lock().await;

                        while let Ok(transcription) = session.transcription_rx.try_recv() {
                            if transcription.text.is_empty() {
                                continue;
                            }

                            let response_payload = serde_json::json!({
                                "transcription": transcription,
                                "timestamp": chrono::Utc::now().timestamp_millis()
                            });

                            let session_id = Uuid::parse_str(session_id);
                            let session_id = match session_id {
                                Ok(session_id) => session_id,
                                Err(_) => {
                                    error!("Failed to parse session ID: {:?}", session_id);
                                    return;
                                }
                            };

                            let response_event = ProcessingEvent {
                                session_id: session_id.to_string(),
                                conversation_id: Uuid::new_v4(),
                                correlation_id: Uuid::new_v4(),
                                event_type: "transcription_output".to_string(),
                                payload: response_payload,
                                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                                source_service: "stt-service".to_string(),
                                target_service: "llm-service".to_string(),
                            };

                            let subject = format!("STT_OUTPUT.session.{}", session_id);

                            if let Err(e) = queue.publish_event(&subject, response_event).await {
                                error!("Failed to publish transcription to LLM service: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub struct SttServiceState {
    pub event_queue: EventQueue,
    pub stt_service: SttService,
}

#[async_trait]
impl ServiceState for SttServiceState {
    fn service_name(&self) -> String {
        "stt-service".to_string()
    }

    fn event_queue(&self) -> &EventQueue {
        &self.event_queue
    }
}
