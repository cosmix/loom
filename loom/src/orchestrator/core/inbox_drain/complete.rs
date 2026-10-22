//! A relayed `complete`: a stage or knowledge session asks the daemon to run
//! its stage's completion on the host. The handler validates who asked and
//! what they saw, queues (or joins) a `Complete` host job, and settles at once
//! with the job id, because a request left `Applying` would be settled
//! `UnknownAfterRestart` on the next pass. The daemon completes the stage
//! only when it applies the job's passing run (`host_verify::apply`); the
//! session follows the job through the state directory (`host_verify::wait`).

use chrono::Utc;

use crate::handoff::completion::content::committed_content;
use crate::host_verify::submit::{submit, SubmitRequest};
use crate::host_verify::JobKind;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::relay::HostJobRequest;

use super::apply::require_owner;
use super::check::{settle_submission, validate_ids};
use super::{InboxHost, Settle};

/// Validate and queue one relayed `complete`.
pub(super) fn apply_complete(
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
    settle_submission(submit(
        host.work_dir(),
        &SubmitRequest {
            stage_id,
            session_id: &record.id,
            request_id,
            kind: JobKind::Complete,
            head: &request.head,
            tree_digest: &request.tree_digest,
            no_cache: request.no_cache,
            criteria: None,
        },
        Utc::now(),
    ))
}

/// Only the live session that owns an executing stage completes it, as a
/// stage session in its worktree or a knowledge session in the repository
/// root.
fn admit_session(host: &dyn InboxHost, stage_id: &str, record: &Session) -> Result<Stage, String> {
    let stage = require_owner(host.work_dir(), stage_id, record)?;
    if !matches!(
        record.session_type,
        SessionType::Stage | SessionType::Knowledge
    ) {
        return Err(format!(
            "a {} session does not complete stage '{stage_id}'",
            record.session_type
        ));
    }
    if stage.status != StageStatus::Executing {
        return Err(format!(
            "stage '{stage_id}' is {}, not executing",
            stage.status
        ));
    }
    Ok(stage)
}

/// The request names well-formed ids, and the checkout's committed content
/// is the HEAD it names: a completion verifies committed content only, so a
/// tree with uncommitted changes is refused here rather than by the run.
fn validate(host: &dyn InboxHost, stage: &Stage, request: &HostJobRequest) -> Result<(), String> {
    validate_ids(request)?;
    let head = committed_content(stage, host.repo_root())
        .map_err(|error| format!("Commit first: {error:#}"))?;
    if head != request.head {
        return Err(format!(
            "the checkout's HEAD is {head}, not {}: run loom stage complete again",
            request.head
        ));
    }
    Ok(())
}
