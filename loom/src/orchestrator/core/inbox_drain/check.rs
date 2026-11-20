//! A relayed `check`: the session asks the daemon to run the stage's gate on
//! the host. The handler validates who asked and what they saw, queues (or
//! joins) a `Check` host job, and settles at once with the job id, because a
//! request left `Applying` would be settled `UnknownAfterRestart` on the next
//! pass. The session follows the job through the state directory
//! (`host_verify::wait`).

use chrono::Utc;

use crate::git::revision::require_head_commit;
use crate::git::worktree::WorktreeGit;
use crate::host_verify::submit::{
    is_lower_hex, submit, submit_at_base, submit_contracts, Submission, SubmitRequest,
};
use crate::host_verify::JobKind;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageType};
use crate::relay::HostJobRequest;
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::trial::refuse_unfrozen;
use crate::verify::criteria::CriteriaConfig;
use crate::verify::transitions::load_stage;

use super::apply::{is_live_judge, require_merge_resolver, require_owner};
use super::commit::pinned_worktree;
use super::{InboxHost, Settle};

#[cfg(test)]
#[path = "check_merge_tests.rs"]
mod merge_tests;

/// Validate and queue one relayed `check`.
pub(super) fn apply_check(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    request: &HostJobRequest,
    request_id: &str,
) -> Settle {
    let stage = match admit_session(host, stage_id, record) {
        Ok(stage) => stage,
        Err(reason) => return Settle::Refused(reason),
    };
    if let Err(reason) = validate(host, &stage, request) {
        return Settle::Refused(reason);
    }
    let submission = SubmitRequest {
        stage_id,
        session_id: &record.id,
        request_id,
        kind: JobKind::Check,
        head: &request.head,
        tree_digest: &request.tree_digest,
        // A resolver's job never reads the pass cache (child_policy.rs), so
        // its request must not join a job that does.
        no_cache: request.no_cache || record.session_type == SessionType::Merge,
        criteria: request.criteria.clone(),
    };
    let (work_dir, now) = (host.work_dir(), Utc::now());
    settle_submission(if request.contracts {
        submit_contracts(work_dir, &submission, &request.replacements, now)
    } else if request.at_base {
        submit_at_base(work_dir, &submission, now)
    } else {
        submit(work_dir, &submission, now)
    })
}

/// Maps a submitted host job to the request's settlement. Shared with the
/// `complete` relay, which settles its submissions the same way.
pub(super) fn settle_submission(submission: anyhow::Result<Submission>) -> Settle {
    match submission {
        Ok(Submission::Queued(job)) => Settle::Applied(Some(format!("host job {} queued", job.id))),
        Ok(Submission::Joined(job)) => Settle::Applied(Some(format!("host job {} joined", job.id))),
        Ok(Submission::Superseded { cancelled, queued }) => Settle::Applied(Some(format!(
            "host job {} queued; {} cancelled",
            queued.id, cancelled.id
        ))),
        Ok(Submission::Waiting { pending, behind }) => Settle::Applied(Some(format!(
            "host job {} queued behind {behind}",
            pending.id
        ))),
        Ok(Submission::Refused(reason)) => Settle::Refused(reason),
        Err(error) => Settle::Refused(format!("could not be applied: {error:#}")),
    }
}

/// A stage or knowledge session must own the stage; a merge session must be
/// resolving this stage's merge; a judge must be the live adjudication session
/// of this stage.
fn admit_session(host: &dyn InboxHost, stage_id: &str, record: &Session) -> Result<Stage, String> {
    match record.session_type {
        SessionType::Merge => return require_merge_resolver(host.work_dir(), stage_id, record),
        SessionType::Adjudication => {}
        _ => return require_owner(host.work_dir(), stage_id, record),
    }
    if !is_live_judge(host, record) {
        return Err(format!(
            "only the live adjudication session for stage '{stage_id}' may request its check"
        ));
    }
    load_stage(stage_id, host.work_dir())
        .map_err(|error| format!("stage '{stage_id}' could not be loaded: {error:#}"))
}

/// The request's head and tree digest are well-formed hex ids. Shared with the
/// `complete` relay.
pub(super) fn validate_ids(request: &HostJobRequest) -> Result<(), String> {
    if !is_lower_hex(&request.head, &[40, 64]) {
        return Err("head must be 40 or 64 lowercase hex characters".to_string());
    }
    if !is_lower_hex(&request.tree_digest, &[64]) {
        return Err("tree_digest must be 64 lowercase hex characters".to_string());
    }
    Ok(())
}

fn validate(host: &dyn InboxHost, stage: &Stage, request: &HostJobRequest) -> Result<(), String> {
    validate_ids(request)?;
    // The host job would fail on an out-of-range selection, which a session
    // reads as a failing criterion; refuse it here as a bad request instead.
    let only = request
        .criteria
        .as_ref()
        .map(|c| c.iter().copied().collect());
    CriteriaConfig::default()
        .with_only(only)
        .check_selection(stage)
        .map_err(|error| format!("{error:#}"))?;
    if request.contracts {
        validate_contracts(host, stage, request)?;
    }
    if request.at_base && stage.stage_type == StageType::Knowledge {
        return Err(format!(
            "stage '{}' runs on the repository root and has no base to run its criteria at",
            stage.id
        ));
    }
    let head = checkout_head(host, stage)?;
    if head != request.head {
        return Err(format!(
            "the checkout's HEAD is {head}, not {}: run loom check again",
            request.head
        ));
    }
    Ok(())
}

/// A contracts check needs frozen contracts of a `standard` stage, and a
/// trial may replace only files the freeze holds; the child checks the
/// files again against the record it runs with.
fn validate_contracts(
    host: &dyn InboxHost,
    stage: &Stage,
    request: &HostJobRequest,
) -> Result<(), String> {
    let none = || format!("stage '{}' has no frozen contracts to check", stage.id);
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        return Err(none());
    }
    let record = load_freeze(host.work_dir(), &stage.id)
        .map_err(|error| format!("the stage's freeze could not be read: {error:#}"))?
        .ok_or_else(none)?;
    refuse_unfrozen(&record, &request.replacements)
}

/// HEAD of the stage's checkout, read with git pinned to that directory: the
/// stage worktree, or the repository root for a knowledge stage.
fn checkout_head(host: &dyn InboxHost, stage: &Stage) -> Result<String, String> {
    let git = if stage.stage_type == StageType::Knowledge {
        WorktreeGit::discovered(host.repo_root())
    } else {
        pinned_worktree(host.repo_root(), stage)?
    };
    require_head_commit(&git)
        .map_err(|error| format!("the checkout's HEAD could not be read: {error:#}"))
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
