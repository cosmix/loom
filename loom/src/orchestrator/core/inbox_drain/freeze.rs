//! Settling a relayed contract freeze: the request was queued as a `Freeze`
//! host job (`apply` calls `handle_freeze_contracts`), or refused.

use std::path::Path;

use anyhow::Result;

use crate::daemon::Response;
use crate::host_verify::store::job_for_request;
use crate::verify::contracts::refusal;

use super::apply::failed;
use super::Settle;

/// Settle the daemon's answer to a relayed freeze. The daemon freezes the
/// contracts, or parks the stage, when it applies the queued run's result. A
/// refusal parks the stage at once.
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
            park_refused_freeze(work_dir, stage_id, session_id, &message);
            Settle::Refused(message)
        }
        Ok(other) => Settle::Refused(format!("unexpected daemon answer: {other:?}")),
        Err(error) => failed(error),
    }
}

/// A relayed freeze tells its writer to end the turn, so a refusal here would
/// leave the stage `Executing` with no one at work: park it for the operator
/// with the refusal as its reason (`verify::contracts::refusal`). The refusal
/// is settled whether or not the park lands.
fn park_refused_freeze(work_dir: &Path, stage_id: &str, session_id: &str, message: &str) {
    if let Err(error) = refusal::park_refused_relay(work_dir, stage_id, session_id, message) {
        tracing::warn!(
            stage_id = %stage_id,
            session_id = %session_id,
            error = %format!("{error:#}"),
            "Could not park the stage of a refused contract freeze for the operator"
        );
    }
}
