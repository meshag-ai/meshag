use anyhow::Result;
use meshag_shared::{FrameWrapper, SystemFrame};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use crate::pipeline::Pipeline;

pub mod executor;
pub mod metrics;

pub use executor::RunnerExecutor;
pub use metrics::RunnerMetrics;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerStatus {
    Idle,
    Running,
    Paused,
    Stopped,
    Error(String),
}

pub struct Runner {
    pub pipeline: Arc<Pipeline>,
    pub session_id: Uuid,
    pub status: Arc<RwLock<RunnerStatus>>,
    pub metrics: Arc<RwLock<RunnerMetrics>>,
    interrupted: Arc<RwLock<bool>>,
}

impl Runner {
    pub fn new(pipeline: Pipeline) -> Self {
        Self {
            pipeline: Arc::new(pipeline),
            session_id: Uuid::new_v4(),
            status: Arc::new(RwLock::new(RunnerStatus::Idle)),
            metrics: Arc::new(RwLock::new(RunnerMetrics::new())),
            interrupted: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut status = self.status.write().await;
        *status = RunnerStatus::Running;
        drop(status);

        info!(
            pipeline = %self.pipeline.name,
            session_id = %self.session_id,
            "Runner started"
        );

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut status = self.status.write().await;
        *status = RunnerStatus::Stopped;
        drop(status);

        info!(
            pipeline = %self.pipeline.name,
            session_id = %self.session_id,
            "Runner stopped"
        );

        Ok(())
    }

    pub async fn push_frame(&self, frame: FrameWrapper) -> Result<()> {
        self.pipeline.push_frame(frame).await
    }

    pub async fn process_next_frame(&self) -> Result<bool> {
        let status = self.status.read().await;
        if *status != RunnerStatus::Running {
            return Ok(false);
        }
        drop(status);

        let frame_opt = self.pipeline.pop_frame().await;

        if let Some(prioritized_frame) = frame_opt {
            let start_time = std::time::Instant::now();

            if prioritized_frame.priority == 0 {
                self.handle_system_frame(prioritized_frame.frame).await?;
            } else {
                let interrupted = *self.interrupted.read().await;
                if interrupted {
                    return Ok(true);
                }

                self.process_data_frame(prioritized_frame.frame).await?;
            }

            let duration = start_time.elapsed();
            let mut metrics = self.metrics.write().await;
            metrics.record_frame_processed(duration);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handle_system_frame(&self, frame: FrameWrapper) -> Result<()> {
        if let FrameWrapper::System(sys_frame) = frame {
            match sys_frame {
                SystemFrame::StartInterruptionFrame { session_id, .. } => {
                    info!(session_id = %session_id, "Interrupt started");
                    let mut interrupted = self.interrupted.write().await;
                    *interrupted = true;

                    let mut queue = self.pipeline.frame_queue.lock().await;
                    queue.clear();
                }
                SystemFrame::StopInterruptionFrame { session_id } => {
                    info!(session_id = %session_id, "Interrupt stopped");
                    let mut interrupted = self.interrupted.write().await;
                    *interrupted = false;
                }
                SystemFrame::EndFrame { session_id, reason } => {
                    info!(session_id = %session_id, reason = %reason, "Session ended");
                    self.stop().await?;
                }
                SystemFrame::ErrorFrame {
                    session_id,
                    error,
                    recoverable,
                } => {
                    error!(session_id = %session_id, error = %error, recoverable = %recoverable, "Error frame received");
                    if !recoverable {
                        self.stop().await?;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn process_data_frame(&self, mut frame: FrameWrapper) -> Result<()> {
        for processor in &self.pipeline.processors {
            let outputs = processor.process(frame).await?;

            if outputs.is_empty() {
                return Ok(());
            }

            frame = outputs[0].clone();

            for output in outputs.into_iter().skip(1) {
                self.pipeline.push_frame(output).await?;
            }
        }

        Ok(())
    }

    pub async fn get_status(&self) -> RunnerStatus {
        self.status.read().await.clone()
    }

    pub async fn get_metrics(&self) -> RunnerMetrics {
        self.metrics.read().await.clone()
    }
}
