use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RunnerMetrics {
    pub frames_processed: u64,
    pub total_processing_time: Duration,
    pub average_processing_time: Duration,
    pub errors: u64,
}

impl RunnerMetrics {
    pub fn new() -> Self {
        Self {
            frames_processed: 0,
            total_processing_time: Duration::ZERO,
            average_processing_time: Duration::ZERO,
            errors: 0,
        }
    }

    pub fn record_frame_processed(&mut self, duration: Duration) {
        self.frames_processed += 1;
        self.total_processing_time += duration;

        if self.frames_processed > 0 {
            self.average_processing_time =
                self.total_processing_time / self.frames_processed as u32;
        }
    }

    pub fn record_error(&mut self) {
        self.errors += 1;
    }
}

impl Default for RunnerMetrics {
    fn default() -> Self {
        Self::new()
    }
}
