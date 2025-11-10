use anyhow::Result;
use async_trait::async_trait;

use crate::frames::FrameWrapper;

pub mod local;
pub mod remote;

pub use local::LocalProcessor;
pub use remote::RemoteProcessor;

#[async_trait]
pub trait Processor: Send + Sync {
    fn name(&self) -> &str;
    async fn process(&self, frame: FrameWrapper) -> Result<Vec<FrameWrapper>>;
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}
