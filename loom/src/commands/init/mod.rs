//! Initialize the state directory structure for loom orchestration.
//!
//! This module provides the `loom init` command which sets up the workspace,
//! optionally initializes from a plan file, and creates stage files.

mod backend;
mod base_branch;
mod cleanup;
mod execute;
mod git_hooks;
mod plan_setup;

#[cfg(test)]
mod backend_tests;
#[cfg(test)]
mod repair_tests;
#[cfg(test)]
mod tests;

pub use execute::execute;
pub use plan_setup::preflight_plan;
