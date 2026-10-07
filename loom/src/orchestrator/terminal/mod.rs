//! Terminal session management
//!
//! Spawns and manages Claude Code sessions via [`backend::SessionBackend`],
//! which selects between the native and tmux backends according to the
//! persisted `[terminal]` config (`.loom/work/config.toml`).
//!
//! Supports three session types:
//! - Stage sessions: run in isolated worktrees for parallel stage execution
//! - Merge sessions: run in the stage worktree, merging the target into the stage branch
//! - Knowledge sessions: run in main repository for knowledge gathering (no worktree)

pub mod backend;
pub mod emulator;
pub mod native;
mod session_tail;
pub mod tmux;

// Re-export terminal emulator
pub use backend::SessionBackend;
pub use emulator::TerminalEmulator;
pub use session_tail::session_tail;
pub use tmux::TmuxBackend;
