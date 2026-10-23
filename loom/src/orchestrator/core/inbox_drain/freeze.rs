//! Settling a relayed contract freeze: the request was queued as a `Freeze`
//! host job (`apply` calls `handle_freeze_contracts`), or refused.

use std::path::Path;

use anyhow::Result;

use crate::daemon::Response;
use crate::host_verify::store::job_for_request;
use crate::models::stage::StageStatus;
use crate::verify::contracts::refusal::{problem_lines, record_daemon_refusal, RefusalSource};
use crate::verify::transitions::load_stage;

use super::apply::failed;
use super::Settle;

/// Settle the daemon's answer to a relayed freeze. The daemon freezes the
/// contracts, or records the refusal, when it applies the queued run's
/// result. A refusal here is recorded at once.
pub(super) fn settle(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    request_id: &str,
    queued: Result<Response>,
) -> Settle {
    match queued {
        Ok(Response::Ok) => {
            // The stage's current job holds this request once it is queued.
            let job = job_for_request(work_dir, stage_id, request_id)
                .ok()
                .flatten();
            let named = job.map(|job| format!(": host job {}", job.id));
            Settle::Applied(Some(format!(
                "contract run queued{}",
                named.unwrap_or_default()
            )))
        }
        Ok(Response::Error { message }) => {
            match record_refused_freeze(work_dir, stage_id, session_id, &message) {
                Ok(()) => Settle::Refused(message),
                Err(error) => failed(error.context(format!("freeze refused: {message}"))),
            }
        }
        Ok(other) => Settle::Refused(format!("unexpected daemon answer: {other:?}")),
        Err(error) => failed(error),
    }
}

/// A relayed freeze tells its writer to end the turn, so the refusal goes to
/// adjudication: record it for the `dispute_refused_freezes` tick step, which
/// files it as a `freeze-refusal` dispute (`verify::contracts::refusal`).
/// `require_owner` refused every non-owner before the freeze handler ran, so
/// this refusal is the owner's; a stage that is not `Executing` (the handler's
/// status refusal) has no contract phase to dispute and records nothing.
fn record_refused_freeze(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    message: &str,
) -> Result<()> {
    if load_stage(stage_id, work_dir)?.status != StageStatus::Executing {
        return Ok(());
    }
    let problems = problem_lines(message);
    record_daemon_refusal(
        work_dir,
        stage_id,
        session_id,
        &problems,
        RefusalSource::Daemon,
    )
}

#[cfg(test)]
#[path = "freeze_tests.rs"]
mod tests;
