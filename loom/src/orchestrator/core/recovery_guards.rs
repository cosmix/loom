//! Pure-function guards for the recovery sync pass, split out of
//! `recovery.rs` so each can be unit tested without constructing an
//! `Orchestrator`.
//!
//! Two failure modes motivate this module, both traced to a `.loom/work/`
//! state directory that was deleted and recreated (e.g. `loom clean --all`
//! followed by `loom init` for a different plan) while a daemon kept running
//! against its old in-memory execution graph:
//!
//! - [`plan_mismatch`] catches a stage file that shares an ID with a node in
//!   the OLD plan's graph, before that mismatch lets the sync reset and
//!   re-promote the node using stale dependency edges.
//! - [`queued_writeback_verdict`] re-checks a stage file's own dependencies
//!   before the sync writes `Queued` into it on the graph's say-so alone —
//!   the graph can consider a stage ready while the file's real dependencies
//!   are unmet.
//!
//! [`too_young_to_judge`] guards a third, unrelated race: a liveness probe
//! run moments after a session is spawned, before its pid identity file
//! exists, must not be read as proof the session is dead.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::{abort_foreign_state, Orchestrator};

/// Returns `Some(reason)` when `stage`'s own `plan_id` names a plan other
/// than `expected`, the plan the running daemon's execution graph was built
/// from.
///
/// A `None` on either side is never treated as a mismatch: `expected` is
/// `None` outside the daemon (foreground runs, tests, or a `config.toml`
/// with no `plan_id`), and a stage file saved before `plan_id` was stamped
/// onto stage records has no opinion either. Only two concrete, disagreeing
/// values count as evidence.
pub(super) fn plan_mismatch(expected: Option<&str>, stage: &Stage) -> Option<String> {
    let expected = expected?;
    let found = stage.plan_id.as_deref()?;
    if expected == found {
        return None;
    }
    Some(format!(
        "stage '{id}' belongs to plan '{found}' but this daemon's execution graph was built \
         from plan '{expected}'; the state directory was replaced under the running daemon \
         (its .loom/work was reused for a different plan) — aborting without touching its files",
        id = stage.id
    ))
}

/// Outcome of re-checking a stage file's own dependencies before writing the
/// `WaitingForDeps -> Queued` transition the execution graph is requesting.
///
/// The graph and the stage file can disagree about readiness once a state
/// directory has been recreated out from under a running daemon: the graph
/// still holds dependency edges from whichever plan it was built from, so it
/// can consider a stage "ready" while the on-disk stage's real dependencies
/// remain unsatisfied.
pub(super) enum QueuedWriteback {
    /// Dependencies are satisfied; safe to write `Queued` into the file.
    Write,
    /// The graph considers the stage ready, but the stage file's own
    /// dependencies are not (yet) complete and merged. The file must stay
    /// `WaitingForDeps`.
    DependenciesUnmet,
    /// The dependency check itself failed (e.g. a transient git error).
    /// Neither write `Queued` nor conclude the dependencies are unmet.
    CheckFailed(anyhow::Error),
}

/// Runs the same phantom-merge-safe dependency check the spawn path uses
/// (`are_all_dependencies_satisfied_cached`) against the stage as loaded
/// from its own file, independent of whatever the execution graph believes.
pub(super) fn queued_writeback_verdict(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> QueuedWriteback {
    match crate::verify::transitions::are_all_dependencies_satisfied_cached(
        stage,
        work_dir,
        repo_root,
        target_branch,
    ) {
        Ok(true) => QueuedWriteback::Write,
        Ok(false) => QueuedWriteback::DependenciesUnmet,
        Err(error) => QueuedWriteback::CheckFailed(error),
    }
}

/// A session younger than this cannot be judged dead by a failed liveness
/// probe: its pid identity file and terminal window are written
/// asynchronously after spawn, so a probe run inside this window observes
/// "not written yet", not "gone".
pub(super) const ORPHAN_PROBE_GRACE_SECS: i64 = 30;

/// Whether `session` is too young for a failed liveness probe to be treated
/// as evidence that it is dead.
pub(super) fn too_young_to_judge(session: &Session, now: DateTime<Utc>) -> bool {
    let age = now.signed_duration_since(session.created_at);
    age < chrono::Duration::seconds(ORPHAN_PROBE_GRACE_SECS)
}

/// Logs and returns `true` when `session` is too young for a failed liveness
/// probe to be treated as evidence that it is dead — its pid identity file
/// and terminal window are written asynchronously after spawn, so a probe
/// run inside that window observes "not written yet", not "gone", and
/// requeuing now would spawn a duplicate session into the same worktree.
pub(super) fn skip_too_young_orphan(session: &Session) -> bool {
    let now = Utc::now();
    if !too_young_to_judge(session, now) {
        return false;
    }
    tracing::debug!(
        session_id = %session.id,
        age_secs = (now - session.created_at).num_seconds(),
        "Orphan probe found no live session, but it is too young to judge; skipping this pass"
    );
    true
}

impl Orchestrator {
    /// A state directory replaced under this running daemon (e.g. `loom
    /// clean --all` then `loom init` for a different plan) leaves this
    /// daemon's flock held on an unlinked inode while a stage file from the
    /// new plan reuses an ID from the old one. Resetting/promoting such a
    /// node with the OLD graph's dependency edges is exactly the bug this
    /// guards against — abort without any by-path cleanup, which would
    /// otherwise corrupt the new owner's state (see `abort_foreign_state`).
    pub(super) fn abort_if_foreign_plan(&self, stage: &Stage) {
        if let Some(reason) = plan_mismatch(self.config.plan_id.as_deref(), stage) {
            abort_foreign_state(&reason);
        }
    }
}
