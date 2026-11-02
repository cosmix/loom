//! Shared classification and guarded stage mutations for merge outcomes.

use anyhow::{ensure, Result};
use chrono::Utc;

use crate::git::merge::{MergeBlock, MergeResult, StashReapply};
use crate::git::signing::CommitTreeError;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::Stage;

/// Engine results retain their evidence while all adapters use one vocabulary.
#[derive(Debug)]
pub(crate) enum MergeDisposition {
    Landed {
        files: u32,
        insertions: u32,
        deletions: u32,
        stash: Option<StashReapply>,
    },
    AlreadyLanded,
    Conflict(Vec<String>),
    Blocked(MergeBlock),
    Held(String),
    MissingBranch,
    MissingWorktree,
}

impl From<MergeResult> for MergeDisposition {
    fn from(result: MergeResult) -> Self {
        match result {
            MergeResult::Success {
                files_changed,
                insertions,
                deletions,
                stash,
            } => Self::Landed {
                files: files_changed,
                insertions,
                deletions,
                stash,
            },
            MergeResult::AlreadyUpToDate => Self::AlreadyLanded,
            MergeResult::Conflict { conflicting_files } => Self::Conflict(conflicting_files),
            MergeResult::Blocked(block) => Self::Blocked(block),
            MergeResult::Held { reason } => Self::Held(reason),
        }
    }
}

pub(crate) enum MergeFailure {
    Signing(String),
    Infrastructure(String),
}

pub(crate) fn classify_failure(stage_id: &str, error: &anyhow::Error) -> MergeFailure {
    if let Some(failure) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CommitTreeError>())
        .filter(|failure| failure.signing)
    {
        return MergeFailure::Signing(signing_reason(stage_id, &failure.detail));
    }
    MergeFailure::Infrastructure(format!("{error:#}"))
}

pub(crate) fn signing_reason(stage_id: &str, detail: &str) -> String {
    format!("merge commit signing failed: {detail}; fix signing (gpg-agent passphrase cache, GUI pinentry or ssh-agent key), then loom stage human-review {stage_id} --approve")
}

/// Refuse stale merge results before they touch the current stage.
pub(crate) fn check_claim(stage: &Stage, expected_commit: Option<&str>) -> Result<()> {
    ensure!(
        !stage.merged && !stage.is_final_merged(),
        "stage '{}' is already merged",
        stage.id
    );
    check_completion_commit(stage, expected_commit)
}

pub(crate) fn check_completion_commit(stage: &Stage, expected_commit: Option<&str>) -> Result<()> {
    ensure!(
        stage.completed_commit.as_deref() == expected_commit,
        "completion proof changed during merge handling"
    );
    Ok(())
}

pub(crate) struct MergeClaim {
    status: crate::models::stage::StageStatus,
    session: Option<String>,
    attempt_started_at: Option<chrono::DateTime<Utc>>,
    fix_attempts: u32,
    commit: Option<String>,
}

impl MergeClaim {
    pub(crate) fn capture(stage: &Stage) -> Self {
        Self {
            status: stage.status.clone(),
            session: stage.session.clone(),
            attempt_started_at: stage.attempt_started_at,
            fix_attempts: stage.fix_attempts,
            commit: stage.completed_commit.clone(),
        }
    }

    pub(crate) fn check(&self, current: &Stage) -> Result<()> {
        check_claim(current, self.commit.as_deref())?;
        ensure!(
            current.status == self.status
                && current.session == self.session
                && current.attempt_started_at == self.attempt_started_at
                && current.fix_attempts == self.fix_attempts,
            "stage ownership changed during merge handling"
        );
        Ok(())
    }
}

pub(crate) fn check_stage(current: &Stage, expected: &Stage) -> Result<()> {
    MergeClaim::capture(expected).check(current)
}

/// Conflict handling differs only where manual retry must respect disabled auto-merge.
pub(crate) enum ConflictAuthority {
    Manual,
    Session,
    Daemon,
}

pub(crate) fn apply_conflict(stage: &mut Stage, authority: ConflictAuthority) -> Result<()> {
    match authority {
        ConflictAuthority::Manual
            if stage.status == crate::models::stage::StageStatus::Completed && !stage.merged =>
        {
            Ok(())
        }
        ConflictAuthority::Daemon => {
            stage.enter_merge_conflict();
            Ok(())
        }
        ConflictAuthority::Session => {
            stage.clear_merge_block();
            stage.try_mark_merge_conflict()
        }
        ConflictAuthority::Manual => stage.try_mark_merge_conflict(),
    }
}

pub(crate) fn apply_block(stage: &mut Stage, block: MergeBlock) {
    stage.block_merge(block);
}

pub(crate) fn apply_review(stage: &mut Stage, reason: &str) {
    stage.route_to_review(reason);
}

pub(crate) fn apply_infrastructure_failure(stage: &mut Stage, reason: &str) -> Result<()> {
    if stage.status != crate::models::stage::StageStatus::MergeBlocked
        && stage.try_mark_merge_blocked().is_err()
    {
        ensure!(
            matches!(
                stage.status,
                crate::models::stage::StageStatus::Completed
                    | crate::models::stage::StageStatus::MergeConflict
            ) && !stage.merged,
            "stage '{}' cannot enter MergeBlocked from {}",
            stage.id,
            stage.status
        );
        stage.force_status_with_reason(
            crate::models::stage::StageStatus::MergeBlocked,
            "merge failed",
        );
    }
    stage.merge.block = None;
    record_infrastructure_failure(stage, reason);
    Ok(())
}

/// A retry of a merge the stage may hold a typed block on failed. Git failing
/// says nothing about the block, and without it the daemon would spawn a
/// counted resolver for a merge with no conflict, so it stays; only a stage
/// with no block takes the ordinary failure.
pub(crate) fn apply_retry_failure(stage: &mut Stage, reason: &str) -> Result<()> {
    if stage.status == crate::models::stage::StageStatus::MergeBlocked
        && stage.merge.block.is_some()
    {
        record_infrastructure_failure(stage, reason);
        return Ok(());
    }
    apply_infrastructure_failure(stage, reason)
}

fn record_infrastructure_failure(stage: &mut Stage, reason: &str) {
    stage.replace_failure(FailureInfo {
        failure_type: FailureType::InfrastructureError,
        detected_at: Utc::now(),
        evidence: reason.lines().map(str::to_owned).collect(),
    });
}

#[cfg(test)]
#[path = "merge_outcome_tests.rs"]
mod tests;
