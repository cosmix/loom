//! Stopping merge resolvers the daemon must not leave running: one on a
//! branch the merge gate holds for human review, which stops through the same
//! proof of death as a stale merge writer (`merge_writer_retirement_unproven`),
//! and one whose spawn failed after its process may have started, which stops
//! through the backend's kill and liveness probe as far as its lane lets them
//! observe it (`teardown_proves_gone`).

use std::fmt;
use std::path::Path;

use anyhow::Result;

use crate::fs::session_files::load_session_exact;
use crate::models::session::{Session, SessionBackendKind, SessionExitReason, SessionType};
use crate::models::stage::Stage;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::signals::{find_live_merge_session_for_stage, remove_signal};
use crate::orchestrator::terminal::backend::merge_resolver_worktree;

use super::resolver_attempts::ReservedAttempt;

/// The error context of a resolver spawn that failed after its process may
/// have started, and whose process could not be proven gone. Its attempt
/// stays spent, and `report_merge_spawn_failure` routes the stage to review.
#[derive(Debug)]
pub(super) struct UnstoppedResolver {
    /// The would-be resolver, carrying the identity its spawn gave it.
    pub(super) session: Session,
}

impl fmt::Display for UnstoppedResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "merge resolver session {} may still be running after its spawn failed: loom \
             could not confirm it stopped",
            self.session.id
        )
    }
}

/// What stopping a stage's merge resolvers found: the note a review reason
/// carries, and whether every resolver was proven stopped.
pub(super) struct ResolverStop {
    pub(super) note: String,
    pub(super) proven: bool,
}

impl ResolverStop {
    /// A resolver that may still be running, described by `note`.
    fn unproven(note: String) -> Self {
        Self {
            note,
            proven: false,
        }
    }
}

impl Orchestrator {
    /// Hand the resolver `session` for `stage` to the backend, to run in the
    /// stage worktree, then settle its answer with
    /// [`Self::settle_resolver_spawn`]. A missing worktree fails before the
    /// backend is touched and gives `attempt` back.
    pub(super) fn launch_resolver(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        attempt: ReservedAttempt,
    ) -> Result<Session> {
        let worktree = match merge_resolver_worktree(&self.config.repo_root, &stage.id) {
            Ok(worktree) => worktree,
            Err(error) => {
                // Nothing spawned: dropping `attempt` gives it back, and the
                // signal names no session record.
                if let Err(remove_error) = remove_signal(&session.id, &self.config.work_dir) {
                    tracing::warn!(
                        session_id = %session.id,
                        error = %remove_error,
                        "Failed to remove the signal of a merge resolver that did not spawn"
                    );
                }
                return Err(error);
            }
        };
        let spawned = self.backend.spawn_merge_session_in_worktree(
            stage,
            &worktree,
            session.clone(),
            signal_path,
        );
        self.settle_resolver_spawn(&stage.id, session, spawned, attempt)
    }

    /// Settle the backend's answer to the spawn of resolver `session` for
    /// `stage_id`. A spawned resolver keeps `attempt`. A failed spawn may
    /// have started a process that its backend could not tear down, so the
    /// would-be session is killed by the identity the backend gave it,
    /// [`teardown_proves_gone`] judges whether that proved it gone, and
    /// [`Self::settle_failed_spawn`] acts on the verdict.
    pub(super) fn settle_resolver_spawn(
        &self,
        stage_id: &str,
        mut session: Session,
        spawned: Result<Session>,
        attempt: ReservedAttempt,
    ) -> Result<Session> {
        let error = match spawned {
            Ok(spawned) => {
                attempt.keep();
                return Ok(spawned);
            }
            Err(error) => error.context("Failed to spawn merge resolution session"),
        };
        session.assign_to_stage(stage_id.to_string());
        session.backend = self.backend.resolve_lane();
        let killed = self.backend.kill_session(&session).is_ok();
        let confirm_gone = || matches!(self.confirm_session_gone(&session), Ok(true));
        let gone = teardown_proves_gone(session.backend, killed, confirm_gone);
        self.settle_failed_spawn(session, error, attempt, gone)
    }

    /// Settle the failed spawn of resolver `session`, whose teardown proved
    /// it `gone` or did not. Its signal is removed either way, since no
    /// session record names it. Proven gone, it gives `attempt` back and
    /// `error` is retried next tick. Not proven gone, the attempt stays spent
    /// and `error` carries [`UnstoppedResolver`].
    fn settle_failed_spawn(
        &self,
        session: Session,
        error: anyhow::Error,
        attempt: ReservedAttempt,
        gone: bool,
    ) -> Result<Session> {
        if let Err(remove_error) = remove_signal(&session.id, &self.config.work_dir) {
            tracing::warn!(
                session_id = %session.id,
                error = %remove_error,
                "Failed to remove the signal of a merge resolver that did not spawn"
            );
        }
        if gone {
            return Err(error);
        }
        attempt.keep();
        Err(error.context(UnstoppedResolver { session }))
    }

    /// Stop every merge resolver that may be running for `stage_id`, whose
    /// branch the merge gate holds: the tracked one (any tracked kind but
    /// `Stage`) and the one a merge signal names. Returns what the review
    /// reason must add, or `None` when no resolver was found.
    pub(super) fn stop_gated_resolvers(&mut self, stage_id: &str) -> Option<String> {
        self.stop_resolvers_proven(stage_id)
            .map(|stopped| stopped.note)
    }

    /// [`Self::stop_gated_resolvers`], also saying whether every resolver it
    /// found was proven stopped.
    pub(super) fn stop_resolvers_proven(&mut self, stage_id: &str) -> Option<ResolverStop> {
        let mut stops = Vec::new();
        let tracked = self
            .active_sessions
            .get(stage_id)
            .filter(|session| session.session_type != SessionType::Stage)
            .cloned();
        if let Some(session) = &tracked {
            stops.push(self.stop_resolver(stage_id, session));
        }
        match find_live_merge_session_for_stage(stage_id, &self.config.work_dir) {
            Ok(Some(id)) if tracked.as_ref().is_none_or(|session| session.id != id) => {
                stops.push(self.stop_signalled_resolver(stage_id, &id));
            }
            Ok(_) => {}
            Err(error) => stops.push(ResolverStop::unproven(format!(
                "loom could not tell whether a merge resolver is running ({error:#})"
            ))),
        }
        if stops.is_empty() {
            return None;
        }
        let proven = stops.iter().all(|stop| stop.proven);
        let notes: Vec<String> = stops.into_iter().map(|stop| stop.note).collect();
        let steps = self.manual_merge_steps(stage_id);
        let note = self.in_progress_merge_note(stage_id);
        Some(ResolverStop {
            note: format!("{}; {note}. Then {steps}", notes.join("; ")),
            proven,
        })
    }

    /// [`Self::stop_resolver`] for the session a live merge signal names.
    fn stop_signalled_resolver(&mut self, stage_id: &str, session_id: &str) -> ResolverStop {
        match load_session_exact(&self.config.work_dir, session_id) {
            Ok(Some(session)) => self.stop_resolver(stage_id, &session),
            Ok(None) | Err(_) => ResolverStop::unproven(unstopped_note(session_id)),
        }
    }

    /// Kill `session`, a resolver for `stage_id`, and retire it once proven
    /// gone; returns the review-reason note saying which.
    fn stop_resolver(&mut self, stage_id: &str, session: &Session) -> ResolverStop {
        if self.merge_writer_retirement_unproven(session, false) {
            return ResolverStop::unproven(unstopped_note(&session.id));
        }
        self.finish_stale_merge_retirement(stage_id, session, SessionExitReason::OperatorStop);
        ResolverStop {
            note: format!("loom stopped merge resolver session {}", session.id),
            proven: true,
        }
    }
}

/// Whether the teardown of a failed spawn's would-be resolver on `lane`
/// proves it gone. `killed` is whether the backend's kill succeeded, and
/// `confirm_gone` runs the backend's liveness probe until it reads the
/// resolver gone or gives up.
///
/// The probe reads PID identity on every lane. On the native lane with a
/// terminal it then reads the window titled with the session's tracking key,
/// so a launched resolver whose PID was never recorded still reads alive, and
/// a native kill fails only on PID identity the probe reads too, or for want
/// of any PID identity on a native lane with no terminal, which launches
/// nothing: there the probe decides alone. The tmux probe reads nothing else,
/// and a launched pane whose PID was never recorded is seen only through its
/// server's socket, which the kill tears down and fails on while the server
/// survives: there the kill must succeed too.
pub(super) fn teardown_proves_gone(
    lane: SessionBackendKind,
    killed: bool,
    confirm_gone: impl FnOnce() -> bool,
) -> bool {
    let kill_must_succeed = match lane {
        SessionBackendKind::Tmux => true,
        SessionBackendKind::Native => false,
    };
    (killed || !kill_must_succeed) && confirm_gone()
}

fn unstopped_note(session_id: &str) -> String {
    format!(
        "merge resolver session {session_id} may still be running: loom could not confirm it \
         stopped, so stop it by hand"
    )
}

#[cfg(test)]
#[path = "resolver_stop_tests.rs"]
mod tests;
