//! Acceptance Criteria Execution Module
//!
//! This module executes shell commands defined as acceptance criteria in loom plans.
//!
//! # Trust Model
//!
//! Plan files (containing acceptance criteria and setup commands) follow the same trust
//! model as Makefiles, shell scripts, or CI/CD configuration files. They are considered
//! trusted project artifacts that are:
//!
//! - Version controlled alongside application code
//! - Reviewed as part of the normal code review process
//! - Authored by project maintainers or approved contributors
//!
//! Users should treat plan files with the same caution as any executable script:
//! do not run plans from untrusted sources without reviewing their contents.
//!
//! # Security Controls
//!
//! While plan files are trusted, this module implements the following controls to
//! limit the impact of runaway or misbehaving commands:
//!
//! - **Command Timeout**: All commands have a configurable timeout (default 5 minutes)
//!   to prevent indefinite hangs from blocking the orchestration pipeline.
//!
//! - **Explicit Shell Invocation**: Commands are executed via `sh -c` (Unix) or
//!   `cmd /C` (Windows) with the command passed as a single argument, avoiding
//!   shell injection through improper argument splitting.
//!
//! - **Isolated Working Directory**: Commands can be scoped to a specific worktree
//!   directory, limiting their filesystem context.
//!
//! # Timeout Behavior
//!
//! When a command exceeds its timeout:
//! - The process is terminated (SIGKILL on Unix, TerminateProcess on Windows)
//! - The criterion is marked as failed with a timeout-specific error message
//! - Subsequent criteria continue to execute (fail-fast is not the default)

mod config;
mod executor;
mod result;
mod runner;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use config::{CriteriaConfig, DEFAULT_COMMAND_TIMEOUT};
pub use executor::{run_single_criterion, run_single_criterion_with_timeout};
pub use result::{AcceptanceResult, CriterionResult};
pub use runner::{run_acceptance, run_acceptance_with_config};
