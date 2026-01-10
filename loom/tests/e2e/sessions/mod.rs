//! End-to-end tests for session lifecycle and management
//!
//! This module is organized into submodules by test category:
//! - `creation`: Session creation and initialization tests
//! - `status`: Session status transition tests
//! - `context`: Context tracking and exhaustion tests
//! - `lifecycle`: Stage assignment and lifecycle tests
//! - `tests`: Attribute assignment and serialization tests

mod context;
mod creation;
mod lifecycle;
mod status;
mod tests;
