use super::Processor;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::stream::{SplitSink, SplitStream, StreamExt};
use futures::SinkExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::frames::FrameWrapper;

type WsWriter = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type WsReader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct RemoteProcessor {
    name: String,
    ws_url: String,
    writer: Arc<Mutex<Option<WsWriter>>>,
    reader: Arc<Mutex<Option<WsReader>>>,
    timeout_duration: Duration,
}

impl RemoteProcessor {
    pub fn new(name: String, ws_url: String) -> Self {
        Self {
            name,
            ws_url,
            writer: Arc::new(Mutex::new(None)),
            reader: Arc::new(Mutex::new(None)),
            timeout_duration: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout_duration: Duration) -> Self {
        self.timeout_duration = timeout_duration;
        self
    }

    async fn ensure_connected(&self) -> Result<()> {
        let mut writer = self.writer.lock().await;
        let mut reader = self.reader.lock().await;

        if writer.is_none() || reader.is_none() {
            let (ws_stream, _) = connect_async(&self.ws_url).await?;
            let (ws_writer, ws_reader) = ws_stream.split();
            *writer = Some(ws_writer);
            *reader = Some(ws_reader);
        }

        Ok(())
    }
}

#[async_trait]
impl Processor for RemoteProcessor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, frame: FrameWrapper) -> Result<Vec<FrameWrapper>> {
        self.ensure_connected().await?;

        let json = frame.to_json()?;
        let message = Message::Text(json);

        {
            let mut writer = self.writer.lock().await;
            if let Some(ws) = writer.as_mut() {
                ws.send(message).await?;
            } else {
                return Err(anyhow!("WebSocket connection not available"));
            }
        }

        let result = timeout(self.timeout_duration, async {
            let mut reader = self.reader.lock().await;
            if let Some(ws) = reader.as_mut() {
                if let Some(msg) = ws.next().await {
                    match msg? {
                        Message::Text(text) => {
                            let frame = FrameWrapper::from_json(&text)?;
                            Ok(vec![frame])
                        }
                        _ => Err(anyhow!("Unexpected message type")),
                    }
                } else {
                    Err(anyhow!("WebSocket connection closed"))
                }
            } else {
                Err(anyhow!("WebSocket connection not available"))
            }
        })
        .await??;

        Ok(result)
    }

    async fn initialize(&mut self) -> Result<()> {
        self.ensure_connected().await
    }

    async fn cleanup(&mut self) -> Result<()> {
        let mut writer = self.writer.lock().await;
        let mut reader = self.reader.lock().await;

        if let Some(mut ws) = writer.take() {
            let _ = ws.send(Message::Close(None)).await;
        }

        *reader = None;

        Ok(())
    }
}
