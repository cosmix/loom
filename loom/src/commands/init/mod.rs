//! Initialize the state directory structure for loom orchestration.
//!
//! This module provides the `loom init` command which sets up the workspace,
//! optionally initializes from a plan file, and creates stage files.

mod cleanup;
mod execute;
mod plan_setup;
mod work_state;

#[cfg(test)]
mod tests;

pub use execute::execute;
