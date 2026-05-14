//! Terminal session management
//!
//! Spawns and manages Claude Code sessions in native terminal windows
//! via [`native::NativeBackend`].
//!
//! Supports three session types:
//! - Stage sessions: run in isolated worktrees for parallel stage execution
//! - Merge sessions: run in main repository for conflict resolution
//! - Knowledge sessions: run in main repository for knowledge gathering (no worktree)

pub mod emulator;
pub mod native;

// Re-export terminal emulator
pub use emulator::TerminalEmulator;
