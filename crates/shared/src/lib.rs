//! Meshag Shared Library
//!
//! Common types, queue management, and utilities shared across all Meshag services

pub mod queue;

pub use queue::{EventQueue, ProcessingEvent, StreamConfig, StreamMetrics};
