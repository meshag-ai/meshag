use super::Processor;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;

use crate::frames::FrameWrapper;

pub struct LocalProcessor {
    name: String,
    handler: Arc<
        dyn Fn(FrameWrapper) -> BoxFuture<'static, Result<Vec<FrameWrapper>>> + Send + Sync,
    >,
}

impl LocalProcessor {
    pub fn new<F>(name: String, handler: F) -> Self
    where
        F: Fn(FrameWrapper) -> BoxFuture<'static, Result<Vec<FrameWrapper>>> + Send + Sync + 'static,
    {
        Self {
            name,
            handler: Arc::new(handler),
        }
    }
}

#[async_trait]
impl Processor for LocalProcessor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, frame: FrameWrapper) -> Result<Vec<FrameWrapper>> {
        (self.handler)(frame).await
    }
}
