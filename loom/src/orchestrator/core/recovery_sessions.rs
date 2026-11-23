//! Current-session retirement after restart.
use super::orphan_adoption::{
    recover_orphaned_stage, register_live_current_session, session_is_current_for_stage,
};
use super::recovery::load_stage_at_path;
use super::{clear_status_line, Orchestrator};
use crate::fs::session_files::{load_session_exact, validate_session_file_id};
use crate::fs::stage_admission::lock_for_daemon;
use crate::models::{
    session::{Session, SessionType},
    stage::{Stage, StageStatus},
};
use crate::orchestrator::session_registry::{identityless_record_is_dead, Admission};
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::update_stage_at_path;
use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

impl Orchestrator {
    pub(super) fn recover_current_sessions(&mut self) -> Result<usize> {
        self.adopt_orphans_and_log();
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(0);
        }
        let stages = self.index_stages_for_recovery()?;
        let mut recovered = 0;
        for entry in std::fs::read_dir(&sessions_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Some(session) = read_recovery_session(&path) else {
                continue;
            };
            let Some((stage, stage_path)) =
                session.stage_id.as_deref().and_then(|id| stages.get(id))
            else {
                continue;
            };
            if session.session_type == SessionType::Merge
                && !session_is_current_for_stage(stage, &session)
            {
                self.recover_merge_record(stage, &session);
                continue;
            }
            if session_is_current_for_stage(stage, &session)
                && self.recover_current_session(stage, stage_path, &session)?
            {
                recovered += 1;
            }
        }
        Ok(recovered)
    }

    /// Judge and, when it is an orphan, retire `stage`'s current `session`
    /// under the stage's admission lock. A lock this pass cannot take defers
    /// the record to the scheduling passes.
    fn recover_current_session(
        &mut self,
        stage: &Stage,
        stage_path: &Path,
        session: &Session,
    ) -> Result<bool> {
        let _admission = match lock_for_daemon(&self.config.work_dir, &stage.id) {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!(stage_id = %stage.id, error = %format!("{error:#}"),
                    "Cannot take the admission lock during recovery; judging the session later");
                self.deferred_orphans
                    .insert(stage.id.clone(), session.id.clone());
                return Ok(false);
            }
        };
        if !self.current_session_is_orphan(stage, session) {
            return Ok(false);
        }
        let retired = self.retire_current_orphan(stage_path, session)?;
        if retired {
            self.discard_retired_session(&session.id);
        }
        Ok(retired)
    }

    /// Remove the record and signal of a session whose stage was retired.
    fn discard_retired_session(&self, session_id: &str) {
        if let Err(error) = validate_session_file_id(session_id) {
            tracing::warn!(%session_id, %error, "Not removing files of an invalid session id");
            return;
        }
        let work_dir = &self.config.work_dir;
        let record = work_dir.join("sessions").join(format!("{session_id}.md"));
        let _ = std::fs::remove_file(record);
        let signal = work_dir.join("signals").join(format!("{session_id}.md"));
        let _ = std::fs::remove_file(signal);
    }

    /// The caller holds the stage's admission lock.
    fn current_session_is_orphan(&mut self, stage: &Stage, session: &Session) -> bool {
        if !session_is_current_for_stage(stage, session) {
            return false;
        }
        match self.liveness.is_alive(session) {
            Ok(true) => {
                let settled = self.settled_live_session(session);
                register_live_current_session(&mut self.active_sessions, stage, &settled);
                false
            }
            Ok(false) => {
                let dead = identityless_record_is_dead(
                    &self.config.work_dir,
                    session,
                    Admission::Held,
                    Utc::now(),
                );
                if !dead {
                    tracing::debug!(session_id = %session.id,
                        "No live process yet, but the launch is too young to judge; deferring");
                    self.deferred_orphans
                        .insert(stage.id.clone(), session.id.clone());
                }
                dead
            }
            Err(error) => {
                tracing::warn!(session_id = %session.id, %error,
                    "Unknown session liveness during recovery; leaving ownership intact");
                false
            }
        }
    }

    fn retire_current_orphan(&mut self, path: &Path, session: &Session) -> Result<bool> {
        let stage_id = session.stage_id.as_deref().expect("current stage session");
        let branch = crate::git::branch::branch_name_for_stage(stage_id);
        let target = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        // Git probing stays outside the stage lock; ownership is checked again inside it.
        let commits =
            crate::git::branch::commits_ahead_of(&branch, &target, &self.config.repo_root)
                .map_err(|error| error.to_string());
        let mut applied = false;
        let updated = update_stage_at_path(stage_id, path, &self.config.work_dir, |stage| {
            if !session_is_current_for_stage(stage, session) {
                return Ok(());
            }
            applied = self.retire_under_lock(stage, session, &commits, &target)?;
            Ok(())
        })?;
        if applied {
            clear_status_line();
            tracing::warn!(%stage_id, status = ?updated.status, "Recovered orphaned current session");
            self.graph.mark_status(stage_id, updated.status)?;
            self.active_sessions.remove(stage_id);
        }
        Ok(applied)
    }

    /// Retire `stage`'s orphaned current session inside the locked update.
    fn retire_under_lock(
        &self,
        stage: &mut Stage,
        session: &Session,
        commits: &Result<usize, String>,
        target: &str,
    ) -> Result<bool> {
        match stage.status {
            StageStatus::Executing
            | StageStatus::WaitingForInput
            | StageStatus::NeedsHandoff
            | StageStatus::Blocked => {
                let before = stage.clone();
                // `route_orphaned_stage` would otherwise force the status.
                if stage.status == StageStatus::WaitingForInput {
                    crate::verify::contracts::refusal::end_wait(stage)?;
                }
                let work_dir = &self.config.work_dir;
                let manual = self.config.manual_mode;
                let retired = recover_orphaned_stage(
                    stage,
                    session,
                    work_dir,
                    commits.clone(),
                    target,
                    manual,
                );
                if !retired {
                    *stage = before;
                }
                Ok(retired)
            }
            StageStatus::MergeConflict | StageStatus::MergeBlocked => {
                stage.session = None;
                stage.close_reason = Some("Merge session crashed/orphaned".into());
                stage.updated_at = chrono::Utc::now();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl Orchestrator {
    /// Re-judge every record deferred as too young at startup; called at the
    /// start of each scheduling pass.
    pub(super) fn recover_deferred_orphans(&mut self) {
        let deferred: Vec<(String, String)> = self
            .deferred_orphans
            .iter()
            .map(|(stage_id, session_id)| (stage_id.clone(), session_id.clone()))
            .collect();
        for (stage_id, session_id) in deferred {
            if self.recover_deferred_orphan(&stage_id, &session_id)
                && self.deferred_orphans.get(&stage_id) == Some(&session_id)
            {
                self.deferred_orphans.remove(&stage_id);
            }
        }
    }

    /// One deferred record: `true` when its entry is settled and dropped.
    fn recover_deferred_orphan(&mut self, stage_id: &str, session_id: &str) -> bool {
        let work_dir = self.config.work_dir.clone();
        let Ok(_admission) = lock_for_daemon(&work_dir, stage_id) else {
            return false;
        };
        let (path, stage, session) = match load_deferred(&work_dir, stage_id, session_id) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => return true,
            Err(error) => {
                tracing::warn!(%stage_id, %session_id, error = %format!("{error:#}"),
                    "Cannot read a deferred orphan; judging it on a later pass");
                return false;
            }
        };
        match self.liveness.is_alive(&session) {
            Ok(true) => {
                let settled = self.settled_live_session(&session);
                register_live_current_session(&mut self.active_sessions, &stage, &settled);
                true
            }
            Ok(false)
                if identityless_record_is_dead(
                    &work_dir,
                    &session,
                    Admission::Held,
                    Utc::now(),
                ) =>
            {
                match self.retire_current_orphan(&path, &session) {
                    Ok(true) => {
                        self.discard_retired_session(session_id);
                        true
                    }
                    Ok(false) => true,
                    Err(error) => {
                        tracing::warn!(%stage_id, %session_id, error = %format!("{error:#}"),
                            "Cannot retire a deferred orphan; retrying on a later pass");
                        false
                    }
                }
            }
            Ok(false) | Err(_) => false,
        }
    }
}

/// The stage file and current session record of a deferred orphan. `None`
/// when either is gone or the record is no longer the stage's current one.
fn load_deferred(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> Result<Option<(PathBuf, Stage, Session)>> {
    let Some(path) = crate::fs::stage_files::find_stage_file(&work_dir.join("stages"), stage_id)?
    else {
        return Ok(None);
    };
    let stage = load_stage_at_path(&path)?;
    let Some(session) = load_session_exact(work_dir, session_id)? else {
        return Ok(None);
    };
    if !session_is_current_for_stage(&stage, &session) {
        return Ok(None);
    }
    Ok(Some((path, stage, session)))
}

fn read_recovery_session(path: &Path) -> Option<Session> {
    let result = std::fs::read_to_string(path)
        .map_err(anyhow::Error::from)
        .and_then(|content| parse_from_markdown(&content, "Session"));
    match result {
        Ok(session) => Some(session),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "Cannot read recovery session; skipping");
            None
        }
    }
}
