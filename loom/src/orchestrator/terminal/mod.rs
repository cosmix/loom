//! Terminal backend abstraction for session management
//!
//! Provides a unified interface for spawning and managing Claude Code sessions
//! in native terminal windows.
//!
//! Supports three session types:
//! - Stage sessions: run in isolated worktrees for parallel stage execution
//! - Merge sessions: run in main repository for conflict resolution
//! - Knowledge sessions: run in main repository for knowledge gathering (no worktree)

pub mod dispatcher;
pub mod emulator;
pub mod native;

use anyhow::Result;
use std::path::Path;

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Backend type — re-exported from the canonical definition in
/// `crate::plan::schema::execution::BackendType`. Defined once to avoid drift
/// between plan schema and runtime.
pub use crate::plan::schema::execution::BackendType;

/// Trait for terminal backends
///
/// Implementations handle spawning Claude Code sessions in native
/// terminal windows while providing a consistent interface.
pub trait TerminalBackend: Send + Sync {
    /// Spawn a new Claude Code session for the given stage
    ///
    /// Creates a native terminal window and runs the claude command with
    /// the signal file path as the initial prompt.
    /// The session runs in the worktree directory for isolated stage work.
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session>;

    /// Spawn a Claude Code session for merge conflict resolution
    ///
    /// Unlike regular stage sessions that run in isolated worktrees, merge sessions
    /// run in the main repository to resolve conflicts. The session will work in
    /// `repo_root` with the merge signal file guiding conflict resolution.
    ///
    /// # Arguments
    /// * `stage` - The stage whose merge is being resolved
    /// * `session` - A merge session (created with `Session::new_merge`)
    /// * `signal_path` - Path to the merge signal file
    /// * `repo_root` - Path to the main repository (not a worktree)
    fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session>;

    /// Spawn a Claude Code session for base branch conflict resolution
    ///
    /// When a stage has multiple dependencies, loom creates a base branch by merging
    /// all dependency branches. If this merge fails due to conflicts, this method
    /// spawns a session to resolve them. The session runs in the main repository.
    ///
    /// After resolution, the user runs `loom retry {stage_id}` to continue.
    ///
    /// # Arguments
    /// * `stage` - The stage whose base branch creation failed
    /// * `session` - A base conflict session (created with `Session::new_base_conflict`)
    /// * `signal_path` - Path to the base conflict signal file
    /// * `repo_root` - Path to the main repository (not a worktree)
    fn spawn_base_conflict_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session>;

    /// Spawn a Claude Code session for knowledge gathering
    ///
    /// Knowledge stages run in the main repository without creating a worktree.
    /// They don't require commits or merging - their purpose is to populate
    /// the doc/loom/knowledge/ directory with codebase understanding.
    ///
    /// # Arguments
    /// * `stage` - The knowledge stage to execute
    /// * `session` - The session for this execution
    /// * `signal_path` - Path to the knowledge signal file
    /// * `repo_root` - Path to the main repository
    fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session>;

    /// Kill a running session
    fn kill_session(&self, session: &Session) -> Result<()>;

    /// Check if a session is still alive
    fn is_session_alive(&self, session: &Session) -> Result<bool>;

    /// Get the backend type
    fn backend_type(&self) -> BackendType;
}

/// Create a terminal backend based on the specified type
pub fn create_backend(
    backend_type: BackendType,
    work_dir: &Path,
) -> Result<Box<dyn TerminalBackend>> {
    match backend_type {
        BackendType::Native => {
            let backend = native::NativeBackend::new(work_dir.to_path_buf())?;
            Ok(Box::new(backend))
        }
    }
}

// Re-export terminal emulator
pub use emulator::TerminalEmulator;

// BackendType tests live alongside the canonical definition in
// `crate::plan::schema::execution`.
