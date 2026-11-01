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
use crate::verify::transitions::{list_all_stages, update_stage};

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

/// Apply the merge-attribution-owned delta to the current canonical record.
///
/// Attribution and Git inspection happen before this call. The closure only
/// revalidates the fresh status and updates merge fields while holding the stage
/// lock, so unrelated concurrent fields survive.
pub(super) fn reconcile_attributed_stage_record(
    stage_id: &str,
    work_dir: &Path,
) -> Result<(StageStatus, Stage, bool)> {
    let mut prior_status = None;
    let mut status_mutated = false;
    let stage = update_stage(stage_id, work_dir, |stage| {
        prior_status = Some(stage.status.clone());
        match stage.status {
            StageStatus::Completed if stage.is_final_merged() => {} // final: never reopened
            StageStatus::Completed => {
                stage.force_status_with_reason(
                    StageStatus::MergeConflict,
                    "an active merge in the main checkout is attributed to this completed, unmerged stage",
                );
                stage.merge_conflict = true;
                status_mutated = true;
            }
            StageStatus::MergeConflict | StageStatus::MergeBlocked => {
                stage.merge_conflict = true;
            }
            _ => {}
        }
        Ok(())
    })?;
    let prior_status = prior_status
        .ok_or_else(|| anyhow::anyhow!("Attribution update did not inspect stage '{stage_id}'"))?;
    Ok((prior_status, stage, status_mutated))
}

/// Read all sessions from `.loom/work/sessions/`, returning a Vec.
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
    let head_branch = current_branch(repo_root).context("Cannot identify active merge target")?;
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
        // A historical session is evidence only when its source branch still
        // resolves to an active merge head. Missing or unreadable branches
        // leave the exact-commit passes below to decide attribution.
        if stages.iter().any(|s| s.id == stage_id) {
            let head_match = match get_branch_head(source_branch, repo_root) {
                Ok(head) => merge.merge_heads.contains(&head),
                Err(_) => false,
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
            let (prior_status, stage, status_mutated) =
                reconcile_attributed_stage_record(&stage_id, work_dir).with_context(|| {
                    format!("Failed to update attributed stage '{stage_id}' for reconciliation")
                })?;
            if !matches!(
                prior_status,
                StageStatus::Completed | StageStatus::MergeConflict | StageStatus::MergeBlocked
            ) {
                tracing::warn!(
                    stage_id = %stage_id,
                    status = ?prior_status,
                    "Active merge attributed to stage in non-merge-related status; \
                     leaving stage unchanged"
                );
                return Ok(ReconciliationOutcome::AttributedNoOp {
                    stage_id,
                    status: prior_status,
                });
            }

            if status_mutated {
                tracing::error!(
                    stage_id = %stage_id,
                    "Detected active merge for a Completed, unmerged stage; moving \
                     it to MergeConflict."
                );
                return Ok(ReconciliationOutcome::StageMutated {
                    stage_id,
                    prior_status,
                    new_status: StageStatus::MergeConflict,
                });
            }

            Ok(ReconciliationOutcome::AttributedNoOp {
                stage_id,
                status: stage.status,
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
#[path = "merge_attribution/tests.rs"]
mod tests;
