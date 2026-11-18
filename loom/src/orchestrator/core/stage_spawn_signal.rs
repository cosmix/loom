//! The signal-writing step of a stage agent spawn, split out of `stage_spawn.rs`.

use std::path::PathBuf;

use crate::models::failure::FailureType;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::orchestrator::busy::{self, BusyOp};
use crate::orchestrator::signals::{generate_contract_signal, generate_signal_with_skills};
use crate::orchestrator::tick::Phase;

use super::stage_executor::get_dependency_status;
use super::stage_spawn::AgentKind;
use super::Orchestrator;

impl Orchestrator {
    /// Write `kind`'s signal for this spawn. Generating it can fail (e.g. an
    /// unwritable signals dir); contain it: mark Blocked rather than
    /// propagating and killing the daemon while the stage is Executing with
    /// no session yet (O-11).
    pub(super) fn write_agent_signal(
        &mut self,
        stage: &Stage,
        worktree: &Worktree,
        session: &Session,
        outgoing: Option<&str>,
        kind: AgentKind,
    ) -> Option<PathBuf> {
        // Building the knowledge pack can take minutes; keep the tick fresh meanwhile.
        let work_dir = self.config.work_dir.clone();
        busy::while_busy(
            &work_dir,
            Phase::Spawning,
            BusyOp::BriefGeneration,
            &stage.id,
            || self.generate_agent_signal(stage, worktree, session, outgoing, kind),
        )
    }

    fn generate_agent_signal(
        &mut self,
        stage: &Stage,
        worktree: &Worktree,
        session: &Session,
        outgoing: Option<&str>,
        kind: AgentKind,
    ) -> Option<PathBuf> {
        let generated = match kind {
            AgentKind::Contract => generate_contract_signal(
                session,
                stage,
                worktree,
                &self.config.work_dir,
                self.skill_index.as_ref(),
                &self.detected_languages,
            ),
            AgentKind::Implementation => {
                let deps = get_dependency_status(stage, &self.graph);
                let handoff_file =
                    self.continuation_handoff_or_block(&stage.id, outgoing, &session.id)?;
                generate_signal_with_skills(
                    session,
                    stage,
                    worktree,
                    &deps,
                    handoff_file.as_deref(),
                    None, // git_history will be extracted from worktree in future enhancement
                    &self.config.work_dir,
                    self.skill_index.as_ref(),
                    &self.detected_languages,
                )
            }
        };
        match generated {
            Ok(path) => Some(path),
            Err(e) => {
                self.block_and_undo_session(
                    &stage.id,
                    &session.id,
                    FailureType::InfrastructureError,
                    format!("Failed to generate signal file: {e:#}"),
                );
                None
            }
        }
    }
}
