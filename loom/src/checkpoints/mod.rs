//! Checkpoint module for task-level progress tracking
//!
//! This module provides:
//! - Checkpoint types and file format
//! - Checkpoint file I/O operations
//! - Task verification execution

mod types;

pub use types::{
    Checkpoint, CheckpointStatus, CheckpointVerificationResult, TaskCompletionRecord,
    TaskDefinition, TaskState, VerificationRule,
};
