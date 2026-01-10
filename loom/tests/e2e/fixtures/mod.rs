//! Test fixture generators for E2E tests
//!
//! Provides pre-built plan content strings with valid loom METADATA blocks.
//!
//! # Submodules
//!
//! - [`plans`]: Plan fixture generators
//! - [`tests`]: Tests for fixture validity

pub mod plans;

#[cfg(test)]
mod tests;

pub use plans::*;
