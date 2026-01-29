use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A Worktree represents a git worktree created for parallel stage execution.
/// Each parallel stage gets its own worktree for file isolation.
/// Worktrees are stored in .worktrees/{stage_id}/ directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    /// Unique identifier (typically matches stage_id)
    pub id: String,
    /// Stage this worktree is for
    pub stage_id: String,
    /// Absolute path to worktree directory
    pub path: PathBuf,
    /// Git branch name (e.g., "loom/stage-1")
    pub branch: String,
    /// Session currently using this worktree
    pub session_id: Option<String>,
    /// Current status
    pub status: WorktreeStatus,
    /// When this worktree was created
    pub created_at: DateTime<Utc>,
    /// When this worktree was last updated
    pub updated_at: DateTime<Utc>,
}

/// Status of a worktree throughout its lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorktreeStatus {
    /// Worktree is being created
    Creating,
    /// Worktree is active and in use
    Active,
    /// Worktree changes are being merged
    Merging,
    /// Worktree has been merged to main
    Merged,
    /// Merge conflict detected
    Conflict,
    /// Worktree has been removed
    Removed,
}

impl Worktree {
    /// Create a new Worktree with Creating status
    ///
    /// # Arguments
    /// * `stage_id` - The stage identifier (also used as worktree id)
    /// * `path` - Absolute path to the worktree directory
    /// * `branch` - Git branch name for this worktree
    pub fn new(stage_id: String, path: PathBuf, branch: String) -> Self {
        let now = Utc::now();
        Self {
            id: stage_id.clone(),
            stage_id,
            path,
            branch,
            session_id: None,
            status: WorktreeStatus::Creating,
            created_at: now,
            updated_at: now,
        }
    }

    /// Assigns or clears the session currently using this worktree
    ///
    /// # Arguments
    /// * `session_id` - The session ID to assign, or None to clear
    pub fn set_session(&mut self, session_id: Option<String>) {
        self.session_id = session_id;
        self.updated_at = Utc::now();
    }

    /// Returns the path to the .work symlink in this worktree
    pub fn get_work_symlink_path(&self) -> PathBuf {
        self.path.join(".work")
    }

    /// Sets the worktree status to Active
    pub fn mark_active(&mut self) {
        self.status = WorktreeStatus::Active;
        self.updated_at = Utc::now();
    }

    /// Sets the worktree status to Merging
    pub fn mark_merging(&mut self) {
        self.status = WorktreeStatus::Merging;
        self.updated_at = Utc::now();
    }

    /// Sets the worktree status to Merged
    pub fn mark_merged(&mut self) {
        self.status = WorktreeStatus::Merged;
        self.updated_at = Utc::now();
    }

    /// Sets the worktree status to Conflict
    pub fn mark_conflict(&mut self) {
        self.status = WorktreeStatus::Conflict;
        self.updated_at = Utc::now();
    }

    /// Sets the worktree status to Removed
    pub fn mark_removed(&mut self) {
        self.status = WorktreeStatus::Removed;
        self.updated_at = Utc::now();
    }

    /// Returns true if the worktree status is Active
    pub fn is_active(&self) -> bool {
        self.status == WorktreeStatus::Active
    }

    /// Returns true if the worktree can be used (Active or Creating)
    pub fn is_available(&self) -> bool {
        matches!(
            self.status,
            WorktreeStatus::Active | WorktreeStatus::Creating
        )
    }

    /// Generates the worktree path for a given stage
    ///
    /// # Arguments
    /// * `base` - The base directory (typically the repository root)
    /// * `stage_id` - The stage identifier
    ///
    /// # Returns
    /// A path in the format "{base}/.worktrees/{stage_id}"
    pub fn worktree_path(base: &Path, stage_id: &str) -> PathBuf {
        base.join(".worktrees").join(stage_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_worktree() {
        let stage_id = "stage-1".to_string();
        let path = PathBuf::from("/repo/.worktrees/stage-1");
        let branch = "loom/stage-1".to_string();

        let worktree = Worktree::new(stage_id.clone(), path.clone(), branch.clone());

        assert_eq!(worktree.id, stage_id);
        assert_eq!(worktree.stage_id, stage_id);
        assert_eq!(worktree.path, path);
        assert_eq!(worktree.branch, branch);
        assert_eq!(worktree.session_id, None);
        assert_eq!(worktree.status, WorktreeStatus::Creating);
    }

    #[test]
    fn test_set_session() {
        let mut worktree = Worktree::new(
            "stage-1".to_string(),
            PathBuf::from("/repo/.worktrees/stage-1"),
            "loom/stage-1".to_string(),
        );

        worktree.set_session(Some("session-123".to_string()));
        assert_eq!(worktree.session_id, Some("session-123".to_string()));

        worktree.set_session(None);
        assert_eq!(worktree.session_id, None);
    }

    #[test]
    fn test_get_work_symlink_path() {
        let worktree = Worktree::new(
            "stage-1".to_string(),
            PathBuf::from("/repo/.worktrees/stage-1"),
            "loom/stage-1".to_string(),
        );

        assert_eq!(
            worktree.get_work_symlink_path(),
            PathBuf::from("/repo/.worktrees/stage-1/.work")
        );
    }

    #[test]
    fn test_status_transitions() {
        let mut worktree = Worktree::new(
            "stage-1".to_string(),
            PathBuf::from("/repo/.worktrees/stage-1"),
            "loom/stage-1".to_string(),
        );

        assert_eq!(worktree.status, WorktreeStatus::Creating);
        assert!(!worktree.is_active());
        assert!(worktree.is_available());

        worktree.mark_active();
        assert_eq!(worktree.status, WorktreeStatus::Active);
        assert!(worktree.is_active());
        assert!(worktree.is_available());

        worktree.mark_merging();
        assert_eq!(worktree.status, WorktreeStatus::Merging);
        assert!(!worktree.is_active());
        assert!(!worktree.is_available());

        worktree.mark_merged();
        assert_eq!(worktree.status, WorktreeStatus::Merged);
        assert!(!worktree.is_active());
        assert!(!worktree.is_available());

        worktree.mark_conflict();
        assert_eq!(worktree.status, WorktreeStatus::Conflict);
        assert!(!worktree.is_active());
        assert!(!worktree.is_available());

        worktree.mark_removed();
        assert_eq!(worktree.status, WorktreeStatus::Removed);
        assert!(!worktree.is_active());
        assert!(!worktree.is_available());
    }

    #[test]
    fn test_worktree_path() {
        let base = Path::new("/repo");
        assert_eq!(
            Worktree::worktree_path(base, "stage-1"),
            PathBuf::from("/repo/.worktrees/stage-1")
        );
        assert_eq!(
            Worktree::worktree_path(base, "my-stage"),
            PathBuf::from("/repo/.worktrees/my-stage")
        );
    }
}
