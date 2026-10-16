//! Current-session retirement after restart.
use super::orphan_adoption::{
    recover_orphaned_stage, register_live_current_session, session_is_current_for_stage,
};
use super::recovery_guards;
use super::{clear_status_line, Orchestrator};
use crate::models::{
    session::Session,
    stage::{Stage, StageStatus},
};
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::update_stage_at_path;
use anyhow::Result;
use std::path::Path;

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
            if !self.current_session_is_orphan(stage, &session) {
                continue;
            }
            if self.retire_current_orphan(stage_path, &session)? {
                recovered += 1;
                let _ = std::fs::remove_file(&path);
                let signal = self
                    .config
                    .work_dir
                    .join("signals")
                    .join(format!("{}.md", session.id));
                let _ = std::fs::remove_file(signal);
            }
        }
        Ok(recovered)
    }

    fn current_session_is_orphan(&mut self, stage: &Stage, session: &Session) -> bool {
        if !session_is_current_for_stage(stage, session) {
            return false;
        }
        match self.liveness.is_alive(session) {
            Ok(true) => {
                register_live_current_session(&mut self.active_sessions, stage, session);
                false
            }
            Ok(false) => !recovery_guards::skip_too_young_orphan(session),
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
            applied = match stage.status {
                StageStatus::Executing | StageStatus::NeedsHandoff | StageStatus::Blocked => {
                    recover_orphaned_stage(
                        stage,
                        session,
                        &self.config.work_dir,
                        commits.clone(),
                        &target,
                        self.config.manual_mode,
                    )
                }
                StageStatus::MergeConflict | StageStatus::MergeBlocked => {
                    stage.session = None;
                    stage.close_reason = Some("Merge session crashed/orphaned".into());
                    stage.updated_at = chrono::Utc::now();
                    true
                }
                _ => false,
            };
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
