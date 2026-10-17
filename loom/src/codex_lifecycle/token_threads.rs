//! Read-only token attribution through the same authorization/job binding as reconciliation.

use std::collections::BTreeSet;
use std::path::Path;

use crate::models::forward_receipt::is_safe_id;
use crate::models::session::Session;

use super::authorization::canonical_daemon_state_root;
use super::jobs::{classify_job, locate_job};
use super::ledger::{read_authorization_rows, LedgerRow};
use super::reconcile::validate_authorization_row;

pub(crate) fn authorized_threads(work_root: &Path, sessions: &[Session]) -> Vec<(String, String)> {
    let Ok(state_root) = canonical_daemon_state_root() else {
        return Vec::new();
    };
    threads_with_state_root(work_root, sessions, &state_root)
}

pub(crate) fn threads_with_state_root(
    work_root: &Path,
    sessions: &[Session],
    state_root: &Path,
) -> Vec<(String, String)> {
    let stages: BTreeSet<_> = sessions
        .iter()
        .filter_map(|session| session.stage_id.as_deref())
        .filter(|stage| is_safe_id(stage))
        .collect();
    let mut threads = BTreeSet::new();
    for stage in stages {
        let path = work_root.join("subagents").join(stage).join("codex.jsonl");
        let Ok(rows) = read_authorization_rows(&path) else {
            continue;
        };
        for row in rows {
            let LedgerRow::Authorization(authorization) = row else {
                continue;
            };
            if authorization.stage_id != stage
                || !sessions.iter().any(|session| {
                    session.stage_id.as_deref() == Some(stage)
                        && session.id == authorization.loom_session_id
                })
                || validate_authorization_row(work_root, &authorization).is_err()
            {
                continue;
            }
            let Ok(job) = locate_job(&authorization, state_root) else {
                continue;
            };
            // Running observations intentionally omit thread IDs for lifecycle purposes;
            // the validated job can already name a rollout with billable usage.
            if classify_job(&job).is_ok() {
                if let Some(thread) = job.thread_id {
                    threads.insert((stage.to_owned(), thread));
                }
            }
        }
    }
    threads.into_iter().collect()
}

#[cfg(test)]
#[path = "token_threads_tests.rs"]
mod tests;
