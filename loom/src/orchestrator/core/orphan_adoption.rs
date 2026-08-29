//! Per-agent orphan adoption: rebuild the session record for one unrecorded
//! live agent, relink it to its stage, and register it as active.
//!
//! Extracted from `recovery.rs` to keep that file under the maintainability
//! limit. Behavior is unchanged from before the move.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::session_registry::{adopt_orphan, OrphanEvidence};
use crate::verify::transitions::update_stage_at_path;

use super::recovery::{load_stage_at_path, scan_stage_paths, Recovery, StageScanCounter};
use super::{clear_status_line, Orchestrator};

pub(super) fn session_is_current_for_stage(stage: &Stage, session: &Session) -> bool {
    stage.session.as_deref() == Some(session.id.as_str())
        && session.stage_id.as_deref() == Some(stage.id.as_str())
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
        let stages_dir = self.config.work_dir.join("stages");
        let mut scan = StageScanCounter::default();
        let mut stages_by_id: HashMap<String, (Stage, PathBuf)> = HashMap::new();
        if stages_dir.exists() {
            for stage_path in scan_stage_paths(&stages_dir, &mut scan)? {
                match load_stage_at_path(&stage_path) {
                    Ok(stage) => {
                        stages_by_id.insert(stage.id.clone(), (stage, stage_path));
                    }
                    Err(error) => tracing::error!(
                        path = %stage_path.display(),
                        error = %error,
                        "Failed to index stage during orphan recovery; skipping"
                    ),
                }
            }
        }
        tracing::debug!(
            directory_reads = scan.directory_reads,
            entries_visited = scan.entries_visited,
            "Indexed current stage sessions for recovery"
        );
        Ok(stages_by_id)
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
