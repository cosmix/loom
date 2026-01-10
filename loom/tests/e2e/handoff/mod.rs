//! E2E tests for context exhaustion detection and handoff automation
//!
//! This module contains tests organized by functionality:
//! - `context_detection`: Tests for context threshold detection and health calculation
//! - `generation`: Tests for handoff file generation and content building
//! - `session_integration`: Tests for session/stage status transitions

pub mod context_detection;
pub mod generation;
pub mod session_integration;
