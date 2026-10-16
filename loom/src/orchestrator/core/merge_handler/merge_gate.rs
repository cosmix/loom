//! Merge gate: refuses to auto-merge, or hand to a merge-resolution session,
//! a stage branch whose diff since it split from the target touches a
//! control path — `.claude/`, `.mcp.json`, `.loom/`, or the tracked git
//! hooks directory. Owner decision 9, `doc/plans/PLAN-loom-state-confinement.md`
//! §9. The path rules live in `crate::git::merge::control_paths`, and
//! `merge_stage` enforces them on the commits it merges. These are the early
//! filters: `try_auto_merge` (before ever attempting the merge, so a
//! conflicting branch never gets that far either) and `resolver_spawn.rs`'s
//! `gate_holds_merge_stage` (for a stage that reached `MergeConflict` or
//! `MergeBlocked` some other way, e.g. `loom stage complete`).

use anyhow::Result;
use chrono::Utc;

use crate::fs::session_files::mark_session_terminal_reason;
use crate::git::merge::control_path_violation;
use crate::models::session::{Session, SessionExitReason, SessionStatus, SessionType};
use crate::orchestrator::core::recovery_guards::too_young_to_judge;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};

impl Orchestrator {
    /// The control paths `is_control_path` matches, worded for operator
    /// messages.
    pub(crate) const CONTROL_PATHS: &'static str =
        ".claude/, .mcp.json, .loom/, or the tracked git hooks directory";

    /// Returns true — after routing `stage_id` to `NeedsHumanReview` whatever
    /// its status — when `stage_branch`'s diff since it split from
    /// `target_branch` touches a control path. The spawn loop's own gate is
    /// `gate_holds_merge_stage`.
    pub(super) fn merge_gate_blocks(
        &mut self,
        stage_id: &str,
        stage_branch: &str,
        target_branch: &str,
    ) -> bool {
        let Some(reason) = self.merge_gate_reason(stage_id, stage_branch, target_branch) else {
            return false;
        };
        self.route_to_human_review(stage_id, reason, None);
        true
    }

    /// The human-review reason when `stage_branch`'s diff since it split from
    /// `target_branch` touches a control path, else `None`. This is an early
    /// filter: a diff that cannot be computed lets the caller go on, and the
    /// gate inside `merge_stage` (which fails closed) decides.
    pub(super) fn merge_gate_reason(
        &self,
        stage_id: &str,
        stage_branch: &str,
        target_branch: &str,
    ) -> Option<String> {
        match control_path_violation(&self.config.repo_root, target_branch, stage_branch) {
            Ok(violation) => violation,
            Err(error) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    %error,
                    "merge gate: failed to compute changed paths; proceeding; merge_stage gates the merge"
                );
                None
            }
        }
    }

    /// Cleans up a stale/dead tracked session for `stage_id` (relocated out
    /// of `spawn_merge_resolution_sessions` to stay within its
    /// maintainability-ledger line budget — unrelated to the gate itself).
    /// Returns true if a live merge/base-conflict resolver session is
    /// already running and the caller should skip spawning a new one.
    ///
    /// When `loom stage complete` detects a merge conflict, the stage
    /// transitions to MergeConflict and `spawn_merge_resolver()` returns
    /// DaemonManaged (no session spawned). However, the original execution
    /// session (`SessionType::Stage`) may still be alive in
    /// `active_sessions` — it hasn't exited yet — and will never resolve the
    /// merge conflict, so it must not block merge resolver spawning: a
    /// tracked Stage session is always stale here and retirement is attempted;
    /// a tracked Merge (or base-conflict) session is left alone while its
    /// process is still alive. Neither is replaced until death is proved: by
    /// PID identity, or without one by the orphan rule
    /// (`identityless_writer_blocks_spawn`).
    pub(super) fn cleanup_stale_merge_session(&mut self, stage_id: &str) -> bool {
        let Some(session) = self.active_sessions.get(stage_id).cloned() else {
            return false;
        };
        if self.merge_writer_retirement_unproven(&session, true) {
            return true;
        }
        self.finish_stale_merge_retirement(stage_id, &session, SessionExitReason::Replaced);
        false
    }

    /// Whether `session`, a merge writer for some stage, is still not proven
    /// gone after its kill. With `spare_live_resolver`, a live resolver (any
    /// tracked kind but `Stage`) is left running and counts as not gone.
    pub(super) fn merge_writer_retirement_unproven(
        &self,
        session: &Session,
        spare_live_resolver: bool,
    ) -> bool {
        let has_identity = matches!(
            session_process_status(&self.config.work_dir, session),
            SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Dead
        );
        let probe = |tracked: &Session| self.backend.is_session_alive(tracked);
        let kill = |stale: &Session| self.backend.kill_session(stale);
        let confirm_gone = |stale: &Session| self.confirm_session_gone(stale);
        if spare_live_resolver {
            stale_merge_retirement_blocks_spawn(session, has_identity, probe, kill, confirm_gone)
        } else {
            retirement_unproven(session, has_identity, probe, kill, confirm_gone)
        }
    }

    /// Retire `session`, a merge writer for `stage_id` proven gone: stop
    /// tracking it if it is the tracked one, remove its signal, and record
    /// `reason` as its exit.
    pub(super) fn finish_stale_merge_retirement(
        &mut self,
        stage_id: &str,
        session: &Session,
        reason: SessionExitReason,
    ) {
        if self
            .active_sessions
            .get(stage_id)
            .is_some_and(|tracked| tracked.id == session.id)
        {
            self.active_sessions.remove(stage_id);
        }
        if let Err(error) = remove_signal(&session.id, &self.config.work_dir) {
            tracing::warn!(session_id = %session.id, %error, "Failed to remove stale signal");
        }
        if let Err(error) = mark_session_terminal_reason(
            &self.config.work_dir,
            &session.id,
            SessionStatus::ContextExhausted,
            reason,
        ) {
            tracing::warn!(
                session_id = %session.id,
                %error,
                "Failed to persist stale merge writer retirement"
            );
        }
    }
}

pub(super) fn stale_merge_retirement_blocks_spawn(
    session: &Session,
    has_pid_identity: bool,
    probe: impl Fn(&Session) -> Result<bool>,
    kill: impl FnOnce(&Session) -> Result<()>,
    confirm_gone: impl FnOnce(&Session) -> Result<bool>,
) -> bool {
    if session.session_type != SessionType::Stage {
        match probe(session) {
            Ok(true) => return true,
            Err(error) => {
                tracing::warn!(
                    session_id = %session.id,
                    %error,
                    "Failed to probe tracked merge writer; retaining ownership"
                );
                return true;
            }
            Ok(false) => {}
        }
    }
    retirement_unproven(session, has_pid_identity, probe, kill, confirm_gone)
}

/// Kill `session` and return whether its death is still not proven: by PID
/// identity when it has one, else by the orphan rule.
fn retirement_unproven(
    session: &Session,
    has_pid_identity: bool,
    probe: impl Fn(&Session) -> Result<bool>,
    kill: impl FnOnce(&Session) -> Result<()>,
    confirm_gone: impl FnOnce(&Session) -> Result<bool>,
) -> bool {
    let kill_error = kill(session).err();
    if !has_pid_identity {
        return identityless_writer_blocks_spawn(session, probe, kill_error.as_ref());
    }
    match confirm_gone(session) {
        Ok(true) => false,
        Ok(false) => warn_retirement_uncertainty(session, kill_error.as_ref(), None),
        Err(error) => warn_retirement_uncertainty(session, kill_error.as_ref(), Some(&error)),
    }
}

/// A stale writer with no verified PID identity can be neither signalled nor
/// confirmed gone by PID, so orphan recovery's rule judges it instead: older
/// than `ORPHAN_PROBE_GRACE_SECS`, a liveness probe that finds it not running
/// proves it dead and it is retired. It keeps ownership while younger (the
/// grace period bounds that), while the backend still sees it running, or
/// while the probe errors, which orphan recovery likewise retries next pass.
fn identityless_writer_blocks_spawn(
    session: &Session,
    probe: impl Fn(&Session) -> Result<bool>,
    kill_error: Option<&anyhow::Error>,
) -> bool {
    let probed = (!too_young_to_judge(session, Utc::now())).then(|| probe(session));
    if matches!(probed, Some(Ok(false))) {
        return false;
    }
    tracing::warn!(
        session_id = %session.id,
        ?kill_error,
        probe = ?probed,
        "Stale merge writer has no verified PID identity and is not proven dead; \
         retaining ownership"
    );
    true
}

fn warn_retirement_uncertainty(
    session: &Session,
    kill_error: Option<&anyhow::Error>,
    confirmation_error: Option<&anyhow::Error>,
) -> bool {
    tracing::warn!(
        session_id = %session.id,
        ?kill_error,
        ?confirmation_error,
        "Failed to prove stale merge writer retirement; retaining ownership"
    );
    true
}

#[cfg(test)]
#[path = "merge_gate_tests.rs"]
mod tests;
