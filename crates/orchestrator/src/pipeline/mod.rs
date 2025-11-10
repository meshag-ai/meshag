use anyhow::Result;
use meshag_shared::{FrameWrapper, Processor};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod builder;
pub mod queue;

pub use builder::PipelineBuilder;

#[derive(Clone)]
pub struct PrioritizedFrame {
    pub frame: FrameWrapper,
    pub priority: u8,
}

impl PartialEq for PrioritizedFrame {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for PrioritizedFrame {}

impl PartialOrd for PrioritizedFrame {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedFrame {
    fn cmp(&self, other: &Self) -> Ordering {
        other.priority.cmp(&self.priority)
    }
}

pub struct Pipeline {
    pub name: String,
    pub processors: Vec<Arc<dyn Processor>>,
    pub frame_queue: Arc<Mutex<BinaryHeap<PrioritizedFrame>>>,
}

impl Pipeline {
    pub fn new(name: String) -> Self {
        Self {
            name,
            processors: Vec::new(),
            frame_queue: Arc::new(Mutex::new(BinaryHeap::new())),
        }
    }

    pub fn builder(name: String) -> PipelineBuilder {
        PipelineBuilder::new(name)
    }

    pub fn add_processor(mut self, processor: Arc<dyn Processor>) -> Self {
        self.processors.push(processor);
        self
    }

    pub async fn push_frame(&self, frame: FrameWrapper) -> Result<()> {
        let priority = frame.frame_category() as u8;
        let prioritized = PrioritizedFrame { frame, priority };

        let mut queue = self.frame_queue.lock().await;
        queue.push(prioritized);

        Ok(())
    }

    pub async fn pop_frame(&self) -> Option<PrioritizedFrame> {
        let mut queue = self.frame_queue.lock().await;
        queue.pop()
    }

    pub async fn initialize(&mut self) -> Result<()> {
        for processor in &mut self.processors {
            Arc::get_mut(processor)
                .ok_or_else(|| anyhow::anyhow!("Cannot get mutable reference to processor"))?
                .initialize()
                .await?;
        }
        Ok(())
    }

    pub async fn cleanup(&mut self) -> Result<()> {
        for processor in &mut self.processors {
            if let Some(proc) = Arc::get_mut(processor) {
                proc.cleanup().await?;
            }
        }
        Ok(())
    }
}
