//! Attribution-aware reconciliation for active main-repo merges.
//!
//! `MERGE_HEAD` in the main repo is global — only one merge in progress at a
//! time across all stages. This module is the single decision point for tying
//! that global state to a specific stage. Without attribution, the recovery
//! code refuses to mutate stage state — never piggybacking a `MergeConflict`
//! revert onto a stage that did not produce the active merge.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::git::branch::{branch_name_for_stage, current_branch, get_branch_head};
use crate::git::merge::{
    detect_in_progress_merge_at, merge_head_exists, ActiveMergeState, InProgressMerge,
};
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::{list_all_stages, load_stage, save_stage};

/// How attribution was reached for an active main-repo merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributionSource {
    /// A live or orphaned `SessionType::Merge` references a matching
    /// `merge_source_branch`.
    MergeSession { session_id: String },
    /// A `MERGE_HEAD` SHA equals the `loom/<stage-id>` branch HEAD.
    BranchHeadMatch,
    /// A `MERGE_HEAD` SHA equals `stage.completed_commit`.
    CompletedCommitMatch,
}

/// Result of attributing a main-repo `MERGE_HEAD` to a stage (or none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeAttribution {
    /// No active main-repo merge.
    None,
    /// Active merge attributed to a specific stage.
    Attributed {
        stage_id: String,
        merge: InProgressMerge,
        source: AttributionSource,
    },
    /// Active merge detected globally but cannot be tied to any known stage.
    /// Includes the `loom/_base/<id>` BaseConflict carve-out.
    GlobalUnattributed(InProgressMerge),
}

/// Outcome of `reconcile_main_repo_active_merge` so callers can update
/// in-memory graphs alongside the on-disk mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    NoActiveMerge,
    /// An attributed stage's status was mutated; caller should re-sync graph.
    StageMutated {
        stage_id: String,
        prior_status: StageStatus,
        new_status: StageStatus,
    },
    /// Active merge could not be attributed; logged, no mutation.
    UnattributedLogged,
    /// Attribution succeeded but stage status disallows mutation; logged only.
    AttributedNoOp {
        stage_id: String,
        status: StageStatus,
    },
}

/// Read all sessions from `.work/sessions/`, returning a Vec.
///
/// Used by the attribution algorithm to find merge metadata. Returns an empty
/// Vec on filesystem errors — attribution then falls back to commit-based
/// matching.
fn load_all_sessions(work_dir: &Path) -> Vec<Session> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(session) = parse_from_markdown::<Session>(&content, "Session") {
            sessions.push(session);
        }
    }
    sessions
}

/// Attribute the main-repo active merge (if any) to a specific stage.
///
/// Algorithm (first match wins):
/// 1. No active merge -> `None`.
/// 2. **BaseConflict carve-out**: HEAD on `loom/_base/*` OR matching
///    BaseConflict session — return `GlobalUnattributed` even if MERGE_HEAD
///    contains a stage branch's commit.
/// 3. `SessionType::Merge` with matching `merge_source_branch` -> `MergeSession`.
/// 4. `loom/<id>` branch HEAD matches a `merge_heads` SHA -> `BranchHeadMatch`.
/// 5. `stage.completed_commit` matches a `merge_heads` SHA -> `CompletedCommitMatch`.
/// 6. Otherwise -> `GlobalUnattributed`.
pub fn attribute_main_repo_merge(
    repo_root: &Path,
    work_dir: &Path,
    stages: &[Stage],
    sessions: &[Session],
) -> Result<MergeAttribution> {
    let merge = match detect_in_progress_merge_at(repo_root)? {
        Some(m) => m,
        None => return Ok(MergeAttribution::None),
    };

    // BaseConflict carve-out:
    //
    // Multi-dependency base merges check out a `loom/_base/<id>` branch and
    // run the merge there. Their MERGE_HEAD may legitimately contain a stage
    // branch's HEAD even though the merge is NOT the stage's merge into the
    // target. Refuse attribution in this case.
    //
    // Detection (heuristic — see knowledge/concerns.md):
    //   * Current branch starts with `loom/_base/`, OR
    //   * Any session has SessionType::BaseConflict matching the current branch.
    let head_branch = current_branch(repo_root).unwrap_or_default();
    if head_branch.starts_with("loom/_base/") {
        return Ok(MergeAttribution::GlobalUnattributed(merge));
    }
    let head_match_base_conflict = sessions.iter().any(|s| {
        s.session_type == SessionType::BaseConflict
            && s.merge_target_branch.as_deref() == Some(&head_branch)
    });
    if head_match_base_conflict {
        return Ok(MergeAttribution::GlobalUnattributed(merge));
    }

    // 1) MergeSession — orphaned or live SessionType::Merge with
    //    merge_source_branch matching loom/<X>.
    for sess in sessions
        .iter()
        .filter(|s| s.session_type == SessionType::Merge)
    {
        let Some(source_branch) = &sess.merge_source_branch else {
            continue;
        };
        // Map the source branch back to a stage id.
        let Some(stage_id) = source_branch.strip_prefix("loom/") else {
            continue;
        };
        // Match by branch HEAD if we can resolve it. Otherwise accept by
        // session metadata alone — recovery_orphaned_sessions might delete
        // before reconcile runs only if order is wrong; we run earlier.
        if stages.iter().any(|s| s.id == stage_id) {
            // Confirm via merge_heads if branch HEAD is resolvable; fall back
            // to metadata-only attribution otherwise.
            let head_match = match get_branch_head(source_branch, repo_root) {
                Ok(head) => merge.merge_heads.contains(&head),
                Err(_) => true,
            };
            if head_match {
                return Ok(MergeAttribution::Attributed {
                    stage_id: stage_id.to_string(),
                    merge,
                    source: AttributionSource::MergeSession {
                        session_id: sess.id.clone(),
                    },
                });
            }
        }
    }

    // 2) BranchHeadMatch — MERGE_HEAD SHA equals a stage's loom/<id> HEAD.
    for stage in stages {
        let branch_name = branch_name_for_stage(&stage.id);
        let head = match get_branch_head(&branch_name, repo_root) {
            Ok(h) => h,
            Err(_) => continue,
        };
        if merge.merge_heads.contains(&head) {
            return Ok(MergeAttribution::Attributed {
                stage_id: stage.id.clone(),
                merge,
                source: AttributionSource::BranchHeadMatch,
            });
        }
    }

    // 3) CompletedCommitMatch — MERGE_HEAD SHA equals stage.completed_commit.
    for stage in stages {
        if let Some(commit) = &stage.completed_commit {
            if merge.merge_heads.contains(commit) {
                return Ok(MergeAttribution::Attributed {
                    stage_id: stage.id.clone(),
                    merge,
                    source: AttributionSource::CompletedCommitMatch,
                });
            }
        }
    }

    // 4) Fall through: detected but unattributable.
    let _ = work_dir; // signature parity with planned API
    Ok(MergeAttribution::GlobalUnattributed(merge))
}

/// Reconcile any main-repo active merge with stage state on disk.
///
/// Pure free function — takes `repo_root` and `work_dir`; mutates only stage
/// files on disk. Daemon-recovery transition tests construct stage files in
/// TempDirs and call this directly.
///
/// On success returns `ReconciliationOutcome` describing what (if anything)
/// was changed; the orchestrator's `reconcile_and_update_graph` wrapper maps
/// that outcome to graph updates.
pub fn reconcile_main_repo_active_merge(
    repo_root: &Path,
    work_dir: &Path,
) -> Result<ReconciliationOutcome> {
    // Cheap fast path: no MERGE_HEAD => no work.
    if !merge_head_exists(repo_root)? {
        return Ok(ReconciliationOutcome::NoActiveMerge);
    }

    let stages = list_all_stages(work_dir).context("Failed to list stages for reconciliation")?;
    let sessions = load_all_sessions(work_dir);

    match attribute_main_repo_merge(repo_root, work_dir, &stages, &sessions)? {
        MergeAttribution::None => Ok(ReconciliationOutcome::NoActiveMerge),
        MergeAttribution::GlobalUnattributed(merge) => {
            tracing::error!(
                location = ?merge.location,
                state = ?merge.state,
                "Active merge in main repo cannot be attributed to any stage. \
                 Loom will refuse merge operations until this is resolved manually."
            );
            Ok(ReconciliationOutcome::UnattributedLogged)
        }
        MergeAttribution::Attributed { stage_id, .. } => {
            let mut stage = load_stage(&stage_id, work_dir).with_context(|| {
                format!("Failed to load attributed stage '{stage_id}' for reconciliation")
            })?;
            let prior_status = stage.status.clone();

            let safe_to_mutate = matches!(
                stage.status,
                StageStatus::Completed | StageStatus::MergeConflict | StageStatus::MergeBlocked
            );
            if !safe_to_mutate {
                tracing::warn!(
                    stage_id = %stage_id,
                    status = ?stage.status,
                    "Active merge attributed to stage in non-merge-related status; \
                     leaving stage unchanged"
                );
                return Ok(ReconciliationOutcome::AttributedNoOp {
                    stage_id,
                    status: prior_status,
                });
            }

            if stage.status == StageStatus::Completed {
                stage.status = StageStatus::MergeConflict;
                stage.merged = false;
                stage.merge_conflict = true;
                save_stage(&stage, work_dir)?;
                tracing::error!(
                    stage_id = %stage_id,
                    "Detected active merge for Completed stage; reverting to \
                     MergeConflict + merged=false (phantom-merge revert)."
                );
                return Ok(ReconciliationOutcome::StageMutated {
                    stage_id,
                    prior_status,
                    new_status: StageStatus::MergeConflict,
                });
            }

            // Already MergeConflict or MergeBlocked: ensure merge_conflict flag is set.
            if !stage.merge_conflict {
                stage.merge_conflict = true;
                save_stage(&stage, work_dir)?;
            }
            Ok(ReconciliationOutcome::AttributedNoOp {
                stage_id,
                status: prior_status,
            })
        }
    }
}

/// Compute the next state for the merge signal text based on `ActiveMergeState`.
pub fn signal_text_state(merge: Option<&InProgressMerge>) -> &'static str {
    match merge.map(|m| &m.state) {
        Some(ActiveMergeState::HasUnmergedPaths(_)) => "in-progress-conflicts",
        Some(ActiveMergeState::ResolvedButUncommitted) => "in-progress-staged",
        None => "fresh-merge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::{Session, SessionStatus, SessionType};
    use crate::models::stage::{Stage, StageStatus, StageType};
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(args: &[&str], cwd: &Path) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        run_git(&["init", "-b", "main"], root);
        run_git(&["config", "user.email", "t@t.com"], root);
        run_git(&["config", "user.name", "t"], root);
        std::fs::write(root.join("README.md"), "seed").unwrap();
        run_git(&["add", "README.md"], root);
        run_git(&["commit", "-m", "seed"], root);
        tmp
    }

    fn make_stage(id: &str) -> Stage {
        let mut s = Stage::new(id.to_string(), Some("test".to_string()));
        s.id = id.to_string();
        s.stage_type = StageType::Standard;
        s.status = StageStatus::Completed;
        s
    }

    fn create_conflict_in_main(repo_root: &Path, stage_id: &str) {
        let branch = format!("loom/{stage_id}");
        run_git(&["checkout", "-b", &branch], repo_root);
        std::fs::write(repo_root.join("file.txt"), "branch\n").unwrap();
        run_git(&["add", "file.txt"], repo_root);
        run_git(&["commit", "-m", "branch"], repo_root);
        run_git(&["checkout", "main"], repo_root);
        std::fs::write(repo_root.join("file.txt"), "main\n").unwrap();
        run_git(&["add", "file.txt"], repo_root);
        run_git(&["commit", "-m", "main"], repo_root);
        // Try to merge -- creates MERGE_HEAD with conflicts.
        let _ = Command::new("git")
            .args(["merge", "--no-ff", &branch])
            .current_dir(repo_root)
            .output()
            .unwrap();
    }

    #[test]
    fn no_active_merge_returns_none() {
        let repo = init_repo();
        let result = attribute_main_repo_merge(repo.path(), repo.path(), &[], &[]).unwrap();
        assert_eq!(result, MergeAttribution::None);
    }

    #[test]
    fn branch_head_match_attributes_to_correct_stage() {
        let repo = init_repo();
        create_conflict_in_main(repo.path(), "stage-a");

        let stage = make_stage("stage-a");
        let result = attribute_main_repo_merge(repo.path(), repo.path(), &[stage], &[]).unwrap();

        match result {
            MergeAttribution::Attributed {
                stage_id, source, ..
            } => {
                assert_eq!(stage_id, "stage-a");
                assert_eq!(source, AttributionSource::BranchHeadMatch);
            }
            other => panic!("expected Attributed, got {other:?}"),
        }
    }

    #[test]
    fn unrelated_stage_b_not_attributed_when_stage_a_is_active() {
        let repo = init_repo();
        create_conflict_in_main(repo.path(), "stage-a");

        let stage_a = make_stage("stage-a");
        let stage_b = make_stage("stage-b"); // no branch
        let result =
            attribute_main_repo_merge(repo.path(), repo.path(), &[stage_b, stage_a], &[]).unwrap();

        match result {
            MergeAttribution::Attributed { stage_id, .. } => {
                assert_eq!(stage_id, "stage-a", "must NOT attribute to stage-b");
            }
            other => panic!("expected Attributed for stage-a, got {other:?}"),
        }
    }

    #[test]
    fn merge_session_attribution_via_metadata() {
        let repo = init_repo();
        create_conflict_in_main(repo.path(), "feat-x");

        let stage = make_stage("feat-x");
        let mut session = Session::new();
        session.id = "merge-session-1".to_string();
        session.status = SessionStatus::Crashed;
        session.session_type = SessionType::Merge;
        session.merge_source_branch = Some("loom/feat-x".to_string());
        session.merge_target_branch = Some("main".to_string());

        let result =
            attribute_main_repo_merge(repo.path(), repo.path(), &[stage], &[session]).unwrap();

        match result {
            MergeAttribution::Attributed {
                stage_id, source, ..
            } => {
                assert_eq!(stage_id, "feat-x");
                match source {
                    AttributionSource::MergeSession { session_id } => {
                        assert_eq!(session_id, "merge-session-1");
                    }
                    other => panic!("expected MergeSession source, got {other:?}"),
                }
            }
            other => panic!("expected Attributed, got {other:?}"),
        }
    }

    #[test]
    fn completed_commit_match_attributes_when_no_branch() {
        let repo = init_repo();
        let root = repo.path();

        // Build a branch and remember its HEAD as completed_commit, then
        // delete the branch. Recreate as a "loom/<id>" with same SHA so we
        // can produce MERGE_HEAD pointing at it.
        run_git(&["checkout", "-b", "loom/stage-c"], root);
        std::fs::write(root.join("c.txt"), "branch\n").unwrap();
        run_git(&["add", "c.txt"], root);
        run_git(&["commit", "-m", "c"], root);
        let stranded_sha = run_git_output(&["rev-parse", "HEAD"], root);

        run_git(&["checkout", "main"], root);
        std::fs::write(root.join("c.txt"), "main\n").unwrap();
        run_git(&["add", "c.txt"], root);
        run_git(&["commit", "-m", "main"], root);

        let _ = Command::new("git")
            .args(["merge", "--no-ff", "loom/stage-c"])
            .current_dir(root)
            .output()
            .unwrap();
        // Now delete the branch reference but MERGE_HEAD is set.
        run_git(&["branch", "-D", "loom/stage-c"], root);

        let mut stage = make_stage("stage-c");
        stage.completed_commit = Some(stranded_sha);

        let result = attribute_main_repo_merge(root, root, &[stage], &[]).unwrap();
        match result {
            MergeAttribution::Attributed {
                stage_id, source, ..
            } => {
                assert_eq!(stage_id, "stage-c");
                assert_eq!(source, AttributionSource::CompletedCommitMatch);
            }
            other => panic!("expected Attributed, got {other:?}"),
        }
    }

    #[test]
    fn base_conflict_branch_carves_out_attribution() {
        let repo = init_repo();
        let root = repo.path();
        // Build a stage branch and bring conflict into a loom/_base branch.
        run_git(&["checkout", "-b", "loom/feat-y"], root);
        std::fs::write(root.join("y.txt"), "y\n").unwrap();
        run_git(&["add", "y.txt"], root);
        run_git(&["commit", "-m", "y"], root);

        run_git(&["checkout", "main"], root);
        run_git(&["checkout", "-b", "loom/_base/composite"], root);
        std::fs::write(root.join("y.txt"), "main\n").unwrap();
        run_git(&["add", "y.txt"], root);
        run_git(&["commit", "-m", "base seed"], root);

        let _ = Command::new("git")
            .args(["merge", "--no-ff", "loom/feat-y"])
            .current_dir(root)
            .output()
            .unwrap();

        let stage = make_stage("feat-y");
        let result = attribute_main_repo_merge(root, root, &[stage], &[]).unwrap();
        match result {
            MergeAttribution::GlobalUnattributed(_) => {}
            other => panic!("expected GlobalUnattributed (BaseConflict carve-out), got {other:?}"),
        }
    }

    fn run_git_output(args: &[&str], cwd: &Path) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}
