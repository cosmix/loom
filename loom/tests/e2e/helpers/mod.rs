//! Test helper functions for E2E tests
//!
//! This module provides common utilities for E2E testing:
//! - Git repository setup
//! - Stage file I/O
//! - Session file I/O
//! - YAML frontmatter parsing
//! - Tmux helpers
//! - Async condition waiting

mod git;
mod session_io;
mod stage_io;
mod utils;
mod yaml;

#[cfg(test)]
mod tests;

// Re-export git helpers
pub use git::{create_temp_git_repo, init_loom_with_plan};

// Re-export stage I/O
pub use stage_io::{create_stage_file, read_stage_file};

// Re-export session I/O
pub use session_io::{create_session_file, read_session_file};

// Re-export utils
pub use utils::{
    cleanup_tmux_sessions, complete_stage, create_signal_file, is_tmux_available,
    wait_for_condition,
};

// Re-export yaml parsing (needed by stage_io and session_io, but also useful for tests)
pub use yaml::extract_yaml_frontmatter;
