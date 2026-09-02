//! Where the disputed criterion runs.
//!
//! Acceptance criteria execute from the stage's worktree root joined with its
//! `working_dir` (`.` when unset) — the same formula
//! `commands/stage/acceptance_runner.rs::resolve_stage_execution_paths` applies
//! and the stage's own signal prints as its "Execution Path". The adjudicator
//! is told to run the criterion itself, so resolving this any other way would
//! send it to a directory where a perfectly good criterion fails — the exact
//! misreading the run is there to prevent.

use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSite {
    /// Directory the criterion runs from.
    pub path: PathBuf,
    /// `working_dir` as the plan states it (`.` when unset).
    pub working_dir: String,
    /// False when the stage's worktree is gone, in which case `path` falls
    /// back to the repository root — a different tree from the disputed one.
    pub worktree_present: bool,
}

impl ExecutionSite {
    pub(super) fn resolve(work_dir: &Path, stage: &Stage) -> Self {
        // The hop count from the state root to the repo root is layout-dependent
        // (two for `.loom/work`, one for a legacy `.work`) and lives in exactly
        // one place: `WorkDir::project_root`. A bare `parent()` here would be
        // right for one layout and wrong for the other.
        let repo_root = WorkDir::new(work_dir)
            .ok()
            .and_then(|wd| wd.project_root().map(Path::to_path_buf))
            .unwrap_or_else(|| work_dir.to_path_buf());
        // `stage.worktree` is the worktree id the executor recorded; it falls
        // back to the stage id, which is what `.worktrees/<stage-id>` uses.
        let worktree = repo_root
            .join(".worktrees")
            .join(stage.worktree.as_deref().unwrap_or(&stage.id));
        let worktree_present = worktree.is_dir();
        let root = if worktree_present {
            worktree
        } else {
            repo_root
        };
        let working_dir = stage.working_dir.clone().unwrap_or_else(|| ".".to_string());
        let path = if working_dir == "." {
            root
        } else {
            root.join(&working_dir)
        };
        Self {
            path,
            working_dir,
            worktree_present,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_in(worktree: Option<&str>, working_dir: Option<&str>) -> Stage {
        Stage {
            id: "s1".to_string(),
            worktree: worktree.map(str::to_string),
            working_dir: working_dir.map(str::to_string),
            ..Stage::default()
        }
    }

    /// The worktree root joined with `working_dir` — the formula acceptance
    /// criteria themselves run under.
    #[test]
    fn resolves_worktree_root_plus_working_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let work = repo.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(repo.join(".worktrees/s1/loom")).unwrap();

        let site = ExecutionSite::resolve(&work, &stage_in(None, Some("loom")));
        assert_eq!(site.path, repo.join(".worktrees/s1/loom"));
        assert_eq!(site.working_dir, "loom");
        assert!(site.worktree_present);
    }

    #[test]
    fn unset_working_dir_is_the_worktree_root() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let work = repo.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(repo.join(".worktrees/s1")).unwrap();

        let site = ExecutionSite::resolve(&work, &stage_in(None, None));
        assert_eq!(site.path, repo.join(".worktrees/s1"));
        assert_eq!(site.working_dir, ".");
    }

    /// A merged or manually cleaned stage has no worktree left; the site falls
    /// back to the repo root and says so, so the briefing can warn.
    #[test]
    fn missing_worktree_falls_back_to_the_repo_root() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let work = repo.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();

        let site = ExecutionSite::resolve(&work, &stage_in(None, None));
        assert_eq!(site.path, repo);
        assert!(!site.worktree_present);
    }

    #[test]
    fn recorded_worktree_id_wins_over_the_stage_id() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let work = repo.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(repo.join(".worktrees/other-id")).unwrap();

        let site = ExecutionSite::resolve(&work, &stage_in(Some("other-id"), None));
        assert_eq!(site.path, repo.join(".worktrees/other-id"));
    }
}
