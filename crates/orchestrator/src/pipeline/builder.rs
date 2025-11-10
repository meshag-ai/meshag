use super::Pipeline;
use anyhow::Result;
use futures::future::BoxFuture;
use meshag_shared::{FrameWrapper, LocalProcessor, Processor, RemoteProcessor};
use std::sync::Arc;

pub struct PipelineBuilder {
    name: String,
    processors: Vec<Arc<dyn Processor>>,
}

impl PipelineBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            processors: Vec::new(),
        }
    }

    pub fn add_processor(mut self, processor: Arc<dyn Processor>) -> Self {
        self.processors.push(processor);
        self
    }

    pub fn add_local_fn<F>(mut self, name: &str, func: F) -> Self
    where
        F: Fn(FrameWrapper) -> BoxFuture<'static, Result<Vec<FrameWrapper>>> + Send + Sync + 'static,
    {
        let processor = LocalProcessor::new(name.to_string(), func);
        self.processors.push(Arc::new(processor));
        self
    }

    pub fn add_remote(mut self, name: &str, ws_url: &str) -> Self {
        let processor = RemoteProcessor::new(name.to_string(), ws_url.to_string());
        self.processors.push(Arc::new(processor));
        self
    }

    pub fn build(self) -> Pipeline {
        Pipeline {
            name: self.name,
            processors: self.processors,
            frame_queue: Arc::new(tokio::sync::Mutex::new(std::collections::BinaryHeap::new())),
        }
    }
}
