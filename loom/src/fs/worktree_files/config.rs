//! Configuration types for worktree file operations

/// Configuration for stage file cleanup
#[derive(Debug, Clone)]
pub struct StageFileCleanupConfig {
    /// Remove session files for the stage
    pub cleanup_sessions: bool,
    /// Remove signal files for the stage
    pub cleanup_signals: bool,
    /// Archive stage file instead of deleting
    pub archive_stage: bool,
    /// Print progress messages
    pub verbose: bool,
}

impl Default for StageFileCleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_sessions: true,
            cleanup_signals: true,
            archive_stage: false,
            verbose: true,
        }
    }
}

impl StageFileCleanupConfig {
    /// Create a quiet config (no verbose output)
    pub fn quiet() -> Self {
        Self {
            verbose: false,
            ..Self::default()
        }
    }

    /// Create a config that archives instead of deleting
    pub fn with_archive() -> Self {
        Self {
            archive_stage: true,
            ..Self::default()
        }
    }
}

/// Result of stage file cleanup
#[derive(Debug, Clone, Default)]
pub struct StageFileCleanupResult {
    /// Number of session files removed
    pub sessions_removed: usize,
    /// Number of signal files removed
    pub signals_removed: usize,
    /// Whether the stage file was archived or removed
    pub stage_file_handled: bool,
    /// Session IDs that were cleaned up
    pub cleaned_session_ids: Vec<String>,
    /// Warnings that occurred during cleanup
    pub warnings: Vec<String>,
}

impl StageFileCleanupResult {
    /// Check if any cleanup was performed
    pub fn any_cleanup_done(&self) -> bool {
        self.sessions_removed > 0 || self.signals_removed > 0 || self.stage_file_handled
    }
}
