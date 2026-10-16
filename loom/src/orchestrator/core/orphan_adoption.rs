//! Orphan handling extracted from `recovery.rs` to keep that file under the
//! maintainability limit: per-agent adoption (rebuild the session record for
//! one unrecorded live agent, relink it to its stage, and register it as
//! active), and the requeue of a stage whose current session died while no
//! daemon watched it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::session_registry::{adopt_orphan, OrphanEvidence};
use crate::verify::transitions::update_stage_at_path;

use super::contract_budget::charge_orphaned_contract_writer;
use super::recovery::Recovery;
use super::{clear_status_line, Orchestrator};

pub(super) fn session_is_current_for_stage(stage: &Stage, session: &Session) -> bool {
    stage.session.as_deref() == Some(session.id.as_str())
        && session.stage_id.as_deref() == Some(stage.id.as_str())
        // An adjudication session carries the stage's own `stage_id` too, but
        // it is never the stage's worker: daemon-restart recovery must not
        // register or delete it as if it were the agent doing the work.
        // Merge sessions ARE current for a stage in `MergeConflict` /
        // `MergeBlocked`, so only `Adjudication` is excluded here.
        && session.session_type != crate::models::session::SessionType::Adjudication
}

pub(super) fn register_live_current_session(
    active_sessions: &mut HashMap<String, Session>,
    stage: &Stage,
    session: &Session,
) -> bool {
    if !session_is_current_for_stage(stage, session) {
        return false;
    }
    active_sessions.insert(stage.id.clone(), session.clone());
    true
}

/// Settle the stage of a current `session` found dead after a daemon
/// restart: requeue it, or route it to handoff when its branch already holds
/// commits past `target_branch`. A `commits_ahead` that failed to count is
/// treated as work: a branch is never discarded on doubt.
///
/// A contract writer is charged to the contract respawn budget first, and
/// its stage goes to `NeedsHumanReview` instead once that budget is spent
/// (see [`charge_orphaned_contract_writer`]) — except in `manual_mode`, where
/// the charge is skipped because loom cannot prove liveness of a writer it
/// did not launch. Returns `false` when that budget cannot be read or
/// charged, leaving the stage as it is: the monitor then reports the dead
/// writer as `ContractSessionEnded`, whose handler retries.
///
/// The charge happens before routing on purpose: a contract writer that died
/// unfrozen spends an attempt whether its stage is requeued or routed to
/// `NeedsHandoff`.
pub(super) fn recover_orphaned_stage(
    stage: &mut Stage,
    session: &Session,
    work_dir: &Path,
    commits_ahead: Result<usize, String>,
    target_branch: &str,
    manual_mode: bool,
) -> bool {
    // Retire stale ownership only. Retry policy belongs to the normal blocked
    // lifecycle, preserving the original reason, retry budget and backoff.
    if super::lifecycle_snapshot::block_cause(stage).is_some() {
        stage.session = None;
        stage.updated_at = chrono::Utc::now();
        return true;
    }
    match charge_orphaned_contract_writer(stage, session, work_dir, manual_mode) {
        Ok(true) => return true,
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(
                stage_id = %stage.id,
                session_id = %session.id,
                %error,
                "Cannot charge an orphaned contract session to the respawn budget; leaving its stage as it is"
            );
            return false;
        }
    }
    route_orphaned_stage(stage, &commits_ahead, target_branch);
    true
}

/// The `close_reason` of a stage routed to handoff.
fn handoff_reason(
    stage_id: &str,
    commits_ahead: &Result<usize, String>,
    target_branch: &str,
) -> String {
    let found = match commits_ahead {
        Ok(count) => format!("branch has {count} commit(s) ahead of {target_branch}"),
        Err(error) => format!(
            "could not count commits ahead of {target_branch} ({error}); \
             treating the branch as holding work"
        ),
    };
    format!(
        "Session orphaned; {found} — needs handoff (use `loom check {stage_id}` to diagnose \
         or `loom stage retry --kill-session {stage_id}` to retry)"
    )
}

fn route_orphaned_stage(
    stage: &mut Stage,
    commits_ahead: &Result<usize, String>,
    target_branch: &str,
) {
    let route_to_handoff = commits_ahead.as_ref().map_or(true, |count| *count > 0);
    if route_to_handoff {
        if let Err(error) = stage.try_mark_needs_handoff() {
            stage.force_status_with_reason(
                StageStatus::NeedsHandoff,
                &format!("orphan recovery (route=handoff): {error}"),
            );
        }
    } else {
        if stage.status == StageStatus::Executing {
            let _ = stage.try_mark_blocked();
        }
        if let Err(error) = stage.try_mark_queued() {
            stage.force_status_with_reason(
                StageStatus::Queued,
                &format!("orphan recovery (route=requeue): {error}"),
            );
        }
    }
    stage.session = None;
    stage.close_reason = Some(if route_to_handoff {
        handoff_reason(&stage.id, commits_ahead, target_branch)
    } else {
        "Session crashed/orphaned".to_string()
    });
    stage.updated_at = chrono::Utc::now();
}

/// Rebuild the session record for one unrecorded-but-live agent and locate
/// its stage file. Returns `None` if adoption cannot proceed — its own
/// warning is already logged, except for a stage that no longer exists at
/// all, which is silently skipped.
fn rebuild_orphan_session(
    work_dir: &Path,
    stages_dir: &Path,
    evidence: &OrphanEvidence,
) -> Option<(Session, PathBuf)> {
    let session = match adopt_orphan(work_dir, evidence) {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!(
                stage_id = %evidence.stage_id,
                session_id = %evidence.session_id,
                %error,
                "Failed to rebuild the session record for an unrecorded live agent"
            );
            return None;
        }
    };

    let stage_path = match crate::fs::stage_files::find_stage_file(stages_dir, &evidence.stage_id) {
        Ok(Some(path)) => path,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(
                stage_id = %evidence.stage_id,
                %error,
                "Failed to locate the stage file for an adopted agent; the session record still stands"
            );
            return None;
        }
    };

    Some((session, stage_path))
}

impl Orchestrator {
    /// Index all current stage files by ID for the orphan-recovery pass.
    pub(super) fn index_stages_for_recovery(&self) -> Result<HashMap<String, (Stage, PathBuf)>> {
        Ok(self.stage_snapshot()?.records)
    }

    /// Re-adopt live agents with no session record, logging once if any were
    /// found. Called FIRST from `recover_orphaned_sessions`: an agent
    /// adopted here has a record by the time the file-driven scan reads the
    /// directory, so the two agree on what is running instead of one of
    /// them concluding the stage is idle.
    pub(super) fn adopt_orphans_and_log(&mut self) {
        let adopted = self.adopt_orphaned_agents();
        if adopted > 0 {
            tracing::info!(
                adopted,
                "Re-adopted live agents that had lost their session records"
            );
        }
    }

    /// Rebuild the session record for one unrecorded-but-live agent, relink
    /// it to its stage under the stage lock, and register it in
    /// `active_sessions`. Returns `true` iff the agent was fully adopted and
    /// is attachable again.
    pub(super) fn try_adopt_orphan(
        &mut self,
        work_dir: &Path,
        stages_dir: &Path,
        evidence: &OrphanEvidence,
    ) -> bool {
        let Some((session, stage_path)) = rebuild_orphan_session(work_dir, stages_dir, evidence)
        else {
            return false;
        };
        self.link_and_register_orphan(work_dir, evidence, &stage_path, session)
    }

    /// Link an adopted agent's session to its stage under the stage lock,
    /// then register it as active. Returns `true` iff both steps succeeded;
    /// every early return leaves the world exactly as it was found (its own
    /// warning already logged, the freshly rebuilt session record left
    /// standing for a later pass to retry).
    fn link_and_register_orphan(
        &mut self,
        work_dir: &Path,
        evidence: &OrphanEvidence,
        stage_path: &Path,
        session: Session,
    ) -> bool {
        let Some(session) = Self::link_orphan_to_stage(work_dir, evidence, stage_path, session)
        else {
            return false;
        };
        self.register_adopted_session(evidence, session)
    }

    /// Re-validate and link an adopted agent's session to its stage under
    /// the stage lock. Returns the session back on success so the caller can
    /// register it, without a second stage-file read.
    ///
    /// The status is deliberately untouched: `Executing` is now true again,
    /// and this pass has no business changing it. A stage that already names
    /// some other session is left alone rather than relinked — that record
    /// is the file-driven pass's to judge, and stealing the link out from
    /// under it would decide the duplicate-agent question here, in the one
    /// place with the least evidence to decide it.
    fn link_orphan_to_stage(
        work_dir: &Path,
        evidence: &OrphanEvidence,
        stage_path: &Path,
        session: Session,
    ) -> Option<Session> {
        let mut linked = false;
        let update = update_stage_at_path(&evidence.stage_id, stage_path, work_dir, |stage| {
            if stage.status == StageStatus::Executing && stage.session.is_none() {
                stage.session = Some(session.id.clone());
                stage.updated_at = chrono::Utc::now();
                linked = true;
            }
            Ok(())
        });
        if let Err(error) = update {
            tracing::warn!(
                stage_id = %evidence.stage_id,
                session_id = %session.id,
                %error,
                "Failed to link an adopted agent to its stage; the session record still stands"
            );
            return None;
        }
        if !linked {
            tracing::warn!(
                stage_id = %evidence.stage_id,
                session_id = %session.id,
                "Adopted an unrecorded live agent but left its stage unlinked (no longer Executing, or already naming another session)"
            );
            return None;
        }
        Some(session)
    }

    /// Register a just-linked orphan session as active. `active_sessions`
    /// holds only a stage's CURRENT session (the invariant
    /// `register_live_current_session` keeps), so this never overwrites: an
    /// existing entry is a session the monitor is already watching, and
    /// replacing it would silently drop that one.
    fn register_adopted_session(&mut self, evidence: &OrphanEvidence, session: Session) -> bool {
        if let Some(existing) = self.active_sessions.get(&evidence.stage_id) {
            tracing::warn!(
                stage_id = %evidence.stage_id,
                adopted_session = %session.id,
                existing_session = %existing.id,
                "Stage already has an active session; not registering the adopted agent"
            );
            return false;
        }

        clear_status_line();
        tracing::warn!(
            stage_id = %evidence.stage_id,
            session_id = %session.id,
            pid = evidence.pid,
            backend = %evidence.backend,
            "Adopted a live agent that had no session record; it is attachable again"
        );
        self.active_sessions
            .insert(evidence.stage_id.clone(), session);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_counted_branch_names_its_commits() {
        let reason = handoff_reason("s", &Ok(3), "main");
        assert!(
            reason.starts_with(
                "Session orphaned; branch has 3 commit(s) ahead of main — needs handoff"
            ),
            "{reason}"
        );
    }

    #[test]
    fn an_uncounted_branch_names_the_error_not_a_count() {
        let reason = handoff_reason("s", &Err("git failed".to_string()), "main");
        assert!(
            reason.contains(
                "could not count commits ahead of main (git failed); \
                 treating the branch as holding work"
            ),
            "{reason}"
        );
        assert!(!reason.contains("commit(s)"), "{reason}");
    }
    #[test]
    fn intentional_blocks_survive_all_orphan_branch_dispositions() {
        let temp = tempfile::TempDir::new().unwrap();
        for reason in ["prerequisite needs operator", "human review rejected"] {
            for ahead in [Ok(0), Ok(2), Err("unknown branch".to_string())] {
                let mut stage = Stage {
                    id: "alpha".into(),
                    status: StageStatus::Blocked,
                    session: Some("old-session".into()),
                    close_reason: Some(reason.into()),
                    ..Default::default()
                };
                assert!(recover_orphaned_stage(
                    &mut stage,
                    &Session::new(),
                    temp.path(),
                    ahead,
                    "main",
                    false
                ));
                assert_eq!(stage.status, StageStatus::Blocked);
                assert_eq!(stage.close_reason.as_deref(), Some(reason));
                assert!(stage.session.is_none());
            }
        }
    }
}
