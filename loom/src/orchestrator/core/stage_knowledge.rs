//! Knowledge-worker session setup and launch.
use super::{persistence::Persistence, stage_executor::get_dependency_status, Orchestrator};
use crate::models::{session::Session, stage::Stage};
use crate::orchestrator::signals::generate_knowledge_signal;
use anyhow::{Context, Result};
use std::path::Path;

impl Orchestrator {
    pub(super) fn launch_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();
        // Persist the session before Executing, so restart always finds its identity.
        let Some(session) = self.write_ahead_knowledge_session(&stage_id)? else {
            return Ok(());
        };
        if !self.setup_knowledge_sandbox_and_hooks(&stage, &stage_id, &session.id)? {
            return Ok(());
        }
        let deps = get_dependency_status(&stage, &self.graph);
        let Some(handoff) =
            self.continuation_handoff_or_block(&stage.id, stage.session.as_deref(), &session.id)
        else {
            return Ok(());
        };
        let signal = generate_knowledge_signal(
            &session,
            &stage,
            &self.config.repo_root,
            &deps,
            &self.config.work_dir,
            handoff.as_deref(),
        )
        .context("Failed to generate knowledge signal file")?;
        let original_id = session.id.clone();
        let Some(spawned) = self.spawn_knowledge_worker(&stage, session, &signal) else {
            return Ok(());
        };
        debug_assert_eq!(
            original_id, spawned.id,
            "Knowledge session identity changed at spawn"
        );
        self.save_session(&spawned)?;
        self.update_stage(&stage_id, |current| {
            current.set_worktree(None);
            current.set_resolved_base(None);
            Ok(())
        })?;
        self.insert_active_session(&stage_id, spawned);
        Ok(())
    }

    fn spawn_knowledge_worker(
        &mut self,
        stage: &Stage,
        session: Session,
        signal: &Path,
    ) -> Option<Session> {
        if self.config.manual_mode {
            for line in super::manual_start::manual_knowledge_start_lines(
                &stage.id,
                &self.config.repo_root,
                signal,
            ) {
                println!("{line}");
            }
            return Some(session);
        }
        let original_id = session.id.clone();
        match self
            .backend
            .spawn_knowledge_session(stage, session, signal, &self.config.repo_root)
        {
            Ok(spawned) => {
                println!("  Started (knowledge): {}", stage.id);
                Some(spawned)
            }
            Err(error) => {
                self.block_and_undo_session(
                    &stage.id,
                    &original_id,
                    super::crash_classification::spawn_failure_type(&error),
                    format!(
                        "Failed to spawn knowledge session for stage {}: {error:#}",
                        stage.id
                    ),
                );
                None
            }
        }
    }
}
