//! Initialize the .work/ directory structure for loom orchestration.
//!
//! This module provides the `loom init` command which sets up the workspace,
//! optionally initializes from a plan file, and creates stage files.

mod cleanup;
mod execute;
mod plan_setup;

#[cfg(test)]
mod tests;

pub use execute::execute;
