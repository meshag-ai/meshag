use super::PrioritizedFrame;
use std::collections::BinaryHeap;

pub struct FrameQueue {
    heap: BinaryHeap<PrioritizedFrame>,
}

impl FrameQueue {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, frame: PrioritizedFrame) {
        self.heap.push(frame);
    }

    pub fn pop(&mut self) -> Option<PrioritizedFrame> {
        self.heap.pop()
    }

    pub fn clear(&mut self) {
        self.heap.clear();
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

impl Default for FrameQueue {
    fn default() -> Self {
        Self::new()
    }
}
