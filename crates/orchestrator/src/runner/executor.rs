use super::{Runner, RunnerStatus};
use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::error;

pub struct RunnerExecutor {
    runner: Runner,
    tick_interval: Duration,
}

impl RunnerExecutor {
    pub fn new(runner: Runner) -> Self {
        Self {
            runner,
            tick_interval: Duration::from_millis(10),
        }
    }

    pub fn with_tick_interval(mut self, interval: Duration) -> Self {
        self.tick_interval = interval;
        self
    }

    pub async fn run(self) -> Result<()> {
        self.runner.start().await?;

        let mut ticker = interval(self.tick_interval);

        loop {
            ticker.tick().await;

            let status = self.runner.get_status().await;
            if status == RunnerStatus::Stopped {
                break;
            }

            if status != RunnerStatus::Running {
                continue;
            }

            match self.runner.process_next_frame().await {
                Ok(had_frame) => {
                    if !had_frame {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }
                Err(e) => {
                    error!("Error processing frame: {}", e);
                }
            }
        }

        Ok(())
    }
}
