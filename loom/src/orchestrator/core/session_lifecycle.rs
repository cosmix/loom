//! Session lifecycle helpers for stage spawn.
//!
//! Extracted from `stage_executor.rs` to keep that file under the
//! maintainability limit. Covers: refusing to spawn a duplicate agent over a
//! live session a crashed daemon lost track of, writing a session's record to
//! disk before the stage is marked Executing, and the Blocked-transition
//! cleanup that undoes an in-flight write-ahead. Behavior is unchanged from
//! before the move.

use anyhow::{Context, Result};
use chrono::Utc;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    /// Refuse to spawn a second agent over one that is still alive. A daemon
    /// crash can leave a stage `Executing` with a session that is
    /// unreachable (e.g. an orphaned tmux server) but still running; if the
    /// stage is later requeued (`loom stage reset`, or any other path that
    /// walks it back to `Queued`), scheduling it again here would spawn a
    /// duplicate agent into the same worktree alongside the first. Adopt the
    /// live session instead of spawning a duplicate.
    ///
    /// Returns `Ok(true)` if a live session was found (and the spawn attempt
    /// should stop here, whether or not the adoption itself fully
    /// succeeded), `Ok(false)` if there is no live session to adopt.
    pub(super) fn adopt_live_session_if_present(&mut self, stage_id: &str) -> Result<bool> {
        let live_sessions = crate::orchestrator::session_registry::live_sessions_for_stage(
            &self.config.work_dir,
            stage_id,
        )?;
        let Some(newest) = live_sessions.into_iter().max_by_key(|s| s.created_at) else {
            return Ok(false);
        };
        let session_id = newest.id.clone();
        tracing::warn!(
            stage_id = %stage_id,
            session_id = %session_id,
            "Adopting live session instead of spawning a duplicate agent"
        );
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.assign_session(session_id.clone());
            if current.status != StageStatus::Executing {
                current.try_mark_executing()?;
                current.begin_attempt(Utc::now());
            }
            Ok(())
        }) {
            tracing::error!(
                stage_id = %stage_id,
                session_id = %session_id,
                error = %e,
                "Failed to adopt live session"
            );
            return Ok(true);
        }
        if let Err(e) = self.graph.mark_executing(stage_id) {
            tracing::warn!(
                stage_id = %stage_id,
                error = %e,
                "Graph state out of sync while adopting a live session"
            );
        }
        self.insert_active_session(stage_id, newest);
        Ok(true)
    }

    /// Resolve the session for this spawn attempt (reusing a pending
    /// recovery signal's session ID if one exists) and write its record to
    /// disk BEFORE the stage is marked Executing.
    ///
    /// Session discovery (`loom attach`, orphan recovery, `loom status`) all
    /// read `.loom/work/sessions/*.md`; writing the record only after the agent is
    /// spawned left a window where a daemon crash produced a live,
    /// unreachable agent with no record on disk at all. Invariant enforced
    /// from here on: a stage is Executing only if `stage.session` names a
    /// session record that exists on disk.
    ///
    /// Returns `None` if the write-ahead failed; the stage has already been
    /// marked Blocked and the caller should return without spawning.
    pub(super) fn write_ahead_session(
        &mut self,
        stage: &Stage,
        stage_id: &str,
    ) -> Option<(Session, Option<(String, std::path::PathBuf)>)> {
        let recovery_signal = self.pending_recovery_signal(stage);
        let mut session = Session::new();
        if let Some((recovery_session_id, _)) = &recovery_signal {
            session.id = recovery_session_id.clone();
        }
        // Populate the identity fields the spawn machinery would otherwise
        // only set inside `prepare_session_launch` at spawn time: the
        // tracking key (derived from stage id + session type) and the lane
        // the spawn is actually going to use. Both need to be right in the
        // record written below, before any spawn has happened.
        session.assign_to_stage(stage_id.to_string());
        session.backend = self.backend.resolve_lane();

        if let Err(e) = self.save_session(&session) {
            let err_msg =
                format!("Failed to write session record ahead of spawn for {stage_id}: {e:#}");
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return None;
        }
        Some((session, recovery_signal))
    }

    /// Resolve the session for a knowledge-stage spawn and write its record
    /// ahead of the stage being marked Executing, mirroring
    /// `write_ahead_session` for worktree stages: a daemon crash between
    /// "Executing" and a live agent must never leave the stage pointing at a
    /// session record that does not exist on disk.
    ///
    /// Returns `Ok(None)` if the stage was marked Blocked instead; the
    /// caller should return without spawning.
    pub(super) fn write_ahead_knowledge_session(
        &mut self,
        stage_id: &str,
    ) -> Result<Option<Session>> {
        let mut session = Session::new_knowledge(stage_id);
        session.backend = self.backend.resolve_lane();
        if let Err(e) = self.save_session(&session) {
            let err_msg = format!(
                "Failed to write session record ahead of spawn for knowledge stage {stage_id}: {e:#}"
            );
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return Ok(None);
        }

        // Mark Executing, linked to the session record above, in ONE locked
        // update so "Executing" and "session assigned" can never disagree.
        let session_id = session.id.clone();
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.try_mark_executing()?;
            current.begin_attempt(Utc::now());
            current.assign_session(session_id.clone());
            Ok(())
        }) {
            self.block_and_undo_session(
                stage_id,
                &session.id,
                FailureType::InfrastructureError,
                format!("Failed to mark knowledge stage executing: {e:#}"),
            );
            return Ok(None);
        }
        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        Ok(Some(session))
    }

    /// Mark a stage Blocked with an `InfrastructureError` after a failure that
    /// occurred *after* the spawn succeeded — a real agent is running under
    /// the session `stage.session` already names (O-11).
    ///
    /// Because the spawn already succeeded, the session record it produced
    /// must be left alone: deleting it or unlinking `stage.session` here would
    /// orphan a live agent instead of a stray file. That is what distinguishes
    /// this from [`Self::block_and_undo_session`], which is for failures
    /// *before* a spawn ever ran. We reload from disk (the in-memory copy may
    /// be stale) and best-effort transition + persist; failures here are
    /// logged, not propagated.
    pub(super) fn block_stranded_stage(&mut self, stage_id: &str, err_msg: String) {
        eprintln!("Stage '{stage_id}' blocked due to spawn-setup failure: {err_msg}");
        match self.persist_blocked_stage(stage_id, FailureType::InfrastructureError, vec![err_msg])
        {
            Ok(()) => {
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
            }
            Err(error) => {
                eprintln!("Failed to persist Blocked state for '{stage_id}': {error:#}");
            }
        }
    }

    /// Mark a stage Blocked and undo an in-flight session write-ahead.
    ///
    /// The spawn paths write `.loom/work/sessions/<id>.md` and link
    /// `stage.session` to it BEFORE a session is actually spawned (see
    /// `write_ahead_session` and `start_knowledge_stage`). A failure between
    /// that write-ahead and a successful spawn — invalid sandbox config, hook
    /// install, signal generation, the spawn call itself — must not leave the
    /// stage pointing at a session record for an agent that never started.
    /// This deletes that record and clears `stage.session` in the SAME locked
    /// update that marks the stage Blocked, so the two can never be observed
    /// apart.
    ///
    /// Only for failures BEFORE a spawn succeeds. A failure after the spawn
    /// has already produced a live agent must use [`Self::block_stranded_stage`]
    /// instead, which leaves the (now real) session record alone.
    pub(super) fn block_and_undo_session(
        &mut self,
        stage_id: &str,
        session_id: &str,
        failure_type: FailureType,
        err_msg: String,
    ) {
        eprintln!("Stage '{stage_id}' blocked: {err_msg}");
        let session_path = self
            .config
            .work_dir
            .join("sessions")
            .join(format!("{session_id}.md"));
        if let Err(e) = std::fs::remove_file(&session_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Warning: failed to remove stranded session record '{session_id}': {e}");
            }
        }
        let result = self.update_stage(stage_id, |current| {
            current.try_mark_blocked()?;
            current.failure_info = Some(FailureInfo {
                failure_type,
                detected_at: Utc::now(),
                evidence: vec![err_msg.clone()],
            });
            current.release_session();
            Ok(())
        });
        match result {
            Ok(_) => {
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
            }
            Err(error) => {
                eprintln!("Failed to persist Blocked state for '{stage_id}': {error:#}");
            }
        }
    }

    /// Insert a spawned session into `active_sessions`, refusing to replace an
    /// existing entry for the stage.
    ///
    /// Silently overwriting here (a plain `.insert()`) is how the daemon
    /// stopped monitoring an original session the moment a second one spawned
    /// into the same worktree: the first entry, and with it the only handle
    /// the daemon had on the live agent, was simply dropped. Once a stage has
    /// a tracked session, only its own removal (completion, crash, merge)
    /// may replace it.
    pub(super) fn insert_active_session(&mut self, stage_id: &str, session: Session) {
        if let Some(existing) = self.active_sessions.get(stage_id) {
            tracing::error!(
                stage_id = %stage_id,
                incumbent_session = %existing.id,
                rejected_session = %session.id,
                "Refusing to evict an already-tracked active session"
            );
            return;
        }
        self.active_sessions.insert(stage_id.to_string(), session);
    }

    /// If the stage's recorded session points at an existing `recovery-*` signal
    /// file, return `(recovery_session_id, signal_path)` so the spawn path can
    /// reuse it and deliver the recovery context (C-5).
    pub(super) fn pending_recovery_signal(
        &self,
        stage: &Stage,
    ) -> Option<(String, std::path::PathBuf)> {
        let session_id = stage.session.as_ref()?;
        if !session_id.starts_with("recovery-") {
            return None;
        }
        let signal_path = self
            .config
            .work_dir
            .join("signals")
            .join(format!("{session_id}.md"));
        if signal_path.exists() {
            Some((session_id.clone(), signal_path))
        } else {
            None
        }
    }

    /// Remove `recovery-<stage_id>-*` signal files that do not belong to the
    /// session about to spawn, so stale recovery signals from prior attempts do
    /// not accumulate in `.loom/work/signals/` (C-5).
    ///
    /// Recovery session IDs are `recovery-<stage_id>-<8hex>-<timestamp>`. We
    /// match the trailing `<8hex>-<timestamp>` shape exactly so a sibling stage
    /// whose ID shares this stage's prefix (e.g. `auth` vs `auth-tests`) is not
    /// caught by a naive `starts_with` — the prefix-collision class behind O-5.
    pub(super) fn cleanup_stale_recovery_signals(&self, stage_id: &str, keep_session_id: &str) {
        let signals_dir = self.config.work_dir.join("signals");
        let prefix = format!("recovery-{stage_id}-");
        let keep_file = format!("{keep_session_id}.md");
        let Ok(entries) = std::fs::read_dir(&signals_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == keep_file {
                continue;
            }
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            let Some(suffix) = stem.strip_prefix(&prefix) else {
                continue;
            };
            // Suffix must be exactly `<8hex>-<digits>` for this stage — not a
            // sibling stage whose ID begins with `stage_id-`.
            if is_recovery_id_suffix(suffix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Whether `suffix` is the `<8hex>-<timestamp>` tail of a recovery session ID.
///
/// Used to distinguish this stage's recovery signals from those of a sibling
/// stage whose ID merely begins with `<stage_id>-`.
fn is_recovery_id_suffix(suffix: &str) -> bool {
    let Some((hex, ts)) = suffix.split_once('-') else {
        return false;
    };
    hex.len() == 8
        && hex.chars().all(|c| c.is_ascii_hexdigit())
        && !ts.is_empty()
        && ts.chars().all(|c| c.is_ascii_digit())
}
