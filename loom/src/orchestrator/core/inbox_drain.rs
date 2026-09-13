//! Daemon side of the relay inbox (`doc/plans/PLAN-loom-state-confinement.md`
//! section 8).
//!
//! Every poll tick, next to the legacy spool drain, this applies what each
//! session's relay hook wrote under `W/inbox/<session-id>/`, retires the relay
//! state of sessions that are gone, and warns about tickets the relay never
//! picked up.
//!
//! Attribution comes from the session RECORD (`W/sessions/<id>.md`), never
//! from the entry: an entry carries the session and stage the relay hook's own
//! environment named, and both must match that record. What a request kind may
//! do for a session kind is `relay::verdict`'s matrix. A refusal is a ledger
//! outcome, never an `Err`; only an I/O failure stops a session's pass, and its
//! entries wait for the next tick. Delivery is at most once: `applying` is
//! recorded before a handler runs, and one found without an outcome is settled
//! `unknown-after-restart`, never applied again.

mod apply;
mod entry;
mod merge_resolved;
mod session_pass;
mod sweep;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_drain;
#[cfg(test)]
mod tests_matrix;
#[cfg(test)]
mod tests_merge;
#[cfg(test)]
mod tests_sweep;

use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::models::session::Session;

use super::Orchestrator;

/// How one drained request was settled: the ledger outcome it becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Settle {
    /// The handler did what was asked; the note, if any, says what that was.
    Applied(Option<String>),
    /// Nothing was done, for the stated reason.
    Refused(String),
}

/// What a drain pass needs from the daemon beyond the state directory.
trait InboxHost {
    fn work_dir(&self) -> &Path;
    fn repo_root(&self) -> &Path;
    /// Whether `session`'s process is alive. `Err` is uncertainty, not death.
    fn session_alive(&self, session: &Session) -> Result<bool>;
    /// Finalize the merge a Merge session resolved for `stage_id`.
    fn resolve_merge(&mut self, session: &Session, stage_id: &str) -> Settle;
    /// True the first time `key` is reported, so a condition that persists
    /// across ticks is logged once rather than every five seconds.
    fn first_report(&mut self, key: &str) -> bool;
    /// Forget `key`, so a later recurrence is reported again.
    fn clear_report(&mut self, key: &str);
}

/// The per-tick inputs a test injects instead of reading the host.
struct Tick<'a> {
    /// The operator-wide scratch root, when it resolves.
    scratch_root: Option<&'a Path>,
    now: DateTime<Utc>,
}

/// What one pass did: every request it settled (by request id, or by file
/// name for an entry refused unread), the sessions it retired and the
/// sessions it warned about.
#[derive(Debug, Default)]
struct PassReport {
    settled: Vec<(String, Settle)>,
    retired: Vec<String>,
    stalled_warned: Vec<String>,
}

impl Orchestrator {
    /// Drain every session inbox, retire the relay state of finished
    /// sessions, and report stalled tickets. Called on every poll tick right
    /// after `drain_stage_spools`.
    ///
    /// `pub` (rather than `pub(super)`) so `tests/integration/relay_e2e.rs`
    /// can run exactly the same pass the daemon's poll tick runs.
    pub fn drain_session_inboxes(&mut self) {
        let scratch_root = match crate::relay::scratch_root_from_env() {
            Ok(root) => Some(root),
            Err(error) => {
                if self.first_report("scratch-root") {
                    tracing::warn!(
                        error = %format!("{error:#}"),
                        "No relay scratch root; stale-ticket checks and scratch cleanup are skipped"
                    );
                }
                None
            }
        };
        let tick = Tick {
            scratch_root: scratch_root.as_deref(),
            now: Utc::now(),
        };
        run_pass(self, &tick);
    }
}

impl InboxHost for Orchestrator {
    fn work_dir(&self) -> &Path {
        &self.config.work_dir
    }

    fn repo_root(&self) -> &Path {
        &self.config.repo_root
    }

    fn session_alive(&self, session: &Session) -> Result<bool> {
        self.liveness.is_alive(session)
    }

    fn resolve_merge(&mut self, session: &Session, stage_id: &str) -> Settle {
        self.resolve_merge_from_inbox(session, stage_id)
    }

    // The spool drain's log-once set, namespaced so neither silences the other.
    fn first_report(&mut self, key: &str) -> bool {
        self.spool_drain_error_logged.insert(format!("inbox:{key}"))
    }

    fn clear_report(&mut self, key: &str) {
        self.spool_drain_error_logged
            .remove(&format!("inbox:{key}"));
    }
}

/// One tick's worth of inbox work: drain first, so a session retired below
/// has already had its entries applied by the ordinary pass.
fn run_pass(host: &mut dyn InboxHost, tick: &Tick<'_>) -> PassReport {
    let mut report = PassReport::default();
    session_pass::drain_inboxes(host, tick, &mut report);
    sweep::sweep_sessions(host, tick, &mut report);
    report
}
