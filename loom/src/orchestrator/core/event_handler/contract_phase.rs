//! Handing a v2 stage from its contract writer to its implementer.
//!
//! A v2 standard stage with contracts runs two agents in turn: a `Contract`
//! session that writes the contract tests and freezes them, then the `Stage`
//! session that implements against them (DESIGN D8). The monitor reports the
//! two ways the first one ends:
//!
//! - `ContractPhaseFinished`: the freeze record exists. The contract writer
//!   idles at its prompt once it has frozen, so loom takes it down (or, in
//!   manual mode, hands it back to the operator) and spawns the implementer.
//! - `ContractSessionEnded`: its process is gone and nothing is frozen. A
//!   fresh contract writer is spawned, up to [`MAX_CONTRACT_RESPAWNS`] times
//!   per stage; then the stage waits for a human.
//!
//! Neither adds a status or a transition edge: the stage is `Executing` from
//! the contract writer's spawn to the implementer's completion.

use anyhow::Result;
use colored::Colorize;

use crate::fs::session_files::{load_session_exact, mark_session_terminal_reason};
use crate::git;
use crate::models::session::{Session, SessionExitReason, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::Worktree;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};
use crate::verify::contracts::store::{attempts_spent, load_freeze, spend_attempt};

use super::super::stage_spawn::AgentKind;
use super::super::{clear_status_line, persistence::Persistence, Orchestrator};

/// Fresh contract writers a stage may be handed after its first one ended
/// without freezing. Spent when handed out.
const MAX_CONTRACT_RESPAWNS: u32 = 3;

impl Orchestrator {
    /// Hand a stage whose contracts are frozen from its contract writer to
    /// its implementer: take the writer down, then spawn the `Stage` session
    /// into the same worktree. A writer that survives the kill keeps the
    /// stage as it is until the next poll asks again.
    pub(super) fn on_contract_phase_finished(
        &mut self,
        stage_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let Some((stage, contract)) = self.current_contract_session(stage_id, session_id)? else {
            return Ok(());
        };
        if !self.retire_contract_writer(stage_id, contract)? {
            return Ok(());
        }
        println!("  Contracts frozen: {stage_id}");
        self.spawn_on_stage_worktree(stage, AgentKind::Implementation)
    }

    /// Take the contract writer off its stage. Returns `false` if it outlived
    /// its kill, and the handover waits for the next poll.
    ///
    /// In manual mode the operator started the writer by hand, so loom holds
    /// no PID identity for it: `take_down_agents` could never confirm it gone
    /// and would defer the handover on every poll. Such a writer is released
    /// to the operator instead. One with identity evidence is taken down.
    fn retire_contract_writer(&mut self, stage_id: &str, contract: Session) -> Result<bool> {
        let unlaunched = self.config.manual_mode
            && session_process_status(&self.config.work_dir, &contract)
                == SessionProcessStatus::Missing;
        if unlaunched {
            self.release_manual_contract_writer(stage_id, &contract)?;
            return Ok(true);
        }
        let agents = self.contract_agents(stage_id, contract);
        let survivors = self.take_down_agents(stage_id, agents, SessionExitReason::Completed)?;
        if survivors.is_empty() {
            return Ok(true);
        }
        clear_status_line();
        eprintln!(
            "{} stage '{stage_id}' froze its contracts, but its contract agent(s) {} \
             survived the kill; the implementation session waits for the next poll.",
            "CONTRACT HANDOVER DEFERRED:".yellow().bold(),
            survivors.join(", ")
        );
        Ok(false)
    }

    /// Close the record of a contract writer loom did not launch, drop its
    /// signal and tracked entry, and tell the operator to end it. The record
    /// ends as the automatic handover leaves it: left in progress, every
    /// later takedown of the stage would find it and, with no identity to
    /// probe, call it a survivor.
    fn release_manual_contract_writer(&mut self, stage_id: &str, contract: &Session) -> Result<()> {
        mark_session_terminal_reason(
            &self.config.work_dir,
            &contract.id,
            SessionStatus::ContextExhausted,
            SessionExitReason::Completed,
        )?;
        self.forget_contract_session(stage_id, &contract.id);
        clear_status_line();
        println!(
            "{} stage '{stage_id}' froze its contracts. Loom did not launch contract session \
             '{}' and cannot end it: exit that Claude session before starting the \
             implementation session below.",
            "CONTRACT HANDOVER:".yellow().bold(),
            contract.id
        );
        Ok(())
    }

    /// Replace a contract writer whose process is gone without a freeze, or,
    /// once [`MAX_CONTRACT_RESPAWNS`] replacements have been handed out, stop
    /// and move the stage to `NeedsHumanReview`.
    pub(super) fn on_contract_session_ended(
        &mut self,
        stage_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let Some((stage, _)) = self.current_contract_session(stage_id, session_id)? else {
            return Ok(());
        };
        let work_dir = self.config.work_dir.clone();
        if load_freeze(&work_dir, stage_id)?.is_some() {
            // Frozen after all: `ContractPhaseFinished` owns this stage.
            return Ok(());
        }
        self.forget_contract_session(stage_id, session_id);
        if attempts_spent(&work_dir, stage_id)? >= MAX_CONTRACT_RESPAWNS {
            return self.escalate_contract_phase(stage_id, session_id);
        }
        let attempt = spend_attempt(&work_dir, stage_id)?;
        clear_status_line();
        eprintln!(
            "Contract session '{session_id}' of stage '{stage_id}' ended without freezing its \
             contracts; starting replacement {attempt} of {MAX_CONTRACT_RESPAWNS}."
        );
        self.spawn_on_stage_worktree(stage, AgentKind::Contract)
    }

    /// The stage and the contract session a report names, re-read from disk:
    /// the stage must still be `Executing` and name `session_id`, and that
    /// record must be this stage's `Contract` session. Anything else means the
    /// stage moved on between the poll and now, and the report is dropped.
    fn current_contract_session(
        &self,
        stage_id: &str,
        session_id: &str,
    ) -> Result<Option<(Stage, Session)>> {
        let stage = self.load_stage(stage_id)?;
        let assigned =
            stage.status == StageStatus::Executing && stage.session.as_deref() == Some(session_id);
        let session = if assigned {
            load_session_exact(&self.config.work_dir, session_id)?
        } else {
            None
        };
        let current = session.filter(|session| {
            session.session_type == SessionType::Contract
                && session.stage_id.as_deref() == Some(stage_id)
        });
        if current.is_none() {
            tracing::debug!(
                stage_id = %stage_id,
                session_id = %session_id,
                "Dropping a contract-phase report: the stage no longer runs that contract session"
            );
        }
        Ok(current.map(|session| (stage, session)))
    }

    /// Every agent the handover must take down: the contract session, and
    /// whatever else the daemon tracks for the stage. `take_down_agents`
    /// stops tracking the stage once nothing survives, so a tracked session
    /// left off this list would lose its only handle without being killed.
    fn contract_agents(&self, stage_id: &str, contract: Session) -> Vec<Session> {
        let tracked = self
            .active_sessions
            .get(stage_id)
            .filter(|tracked| tracked.id != contract.id)
            .cloned();
        std::iter::once(contract).chain(tracked).collect()
    }

    /// Drop the daemon's handles on a contract session it no longer runs:
    /// its tracked entry, so the successor can be tracked in its place, and
    /// its signal file.
    fn forget_contract_session(&mut self, stage_id: &str, session_id: &str) {
        if self
            .active_sessions
            .get(stage_id)
            .is_some_and(|tracked| tracked.id == session_id)
        {
            self.active_sessions.remove(stage_id);
        }
        if let Err(e) = remove_signal(session_id, &self.config.work_dir) {
            eprintln!("Warning: Failed to remove signal for session '{session_id}': {e}");
        }
    }

    /// Every replacement contract writer has ended without freezing: stop
    /// spawning and wait for a human.
    fn escalate_contract_phase(&mut self, stage_id: &str, session_id: &str) -> Result<()> {
        let reason = format!(
            "contract session ended {MAX_CONTRACT_RESPAWNS} times without freezing contracts"
        );
        self.update_stage(stage_id, |stage| {
            if stage.status != StageStatus::Executing
                || stage.session.as_deref() != Some(session_id)
            {
                return Ok(());
            }
            stage.try_request_human_review(reason)?;
            stage.release_session();
            Ok(())
        })?;
        Ok(())
    }

    /// Spawn `kind`'s agent into the stage's existing worktree. The daemon's
    /// handle on it is gone after a restart, but the worktree on disk is not.
    /// A worktree that is gone too held work nothing can recreate, so the
    /// stage is blocked rather than handed a fresh one.
    fn spawn_on_stage_worktree(&mut self, stage: Stage, kind: AgentKind) -> Result<()> {
        let worktree = match self.active_worktrees.get(&stage.id) {
            Some(worktree) => worktree.clone(),
            None => {
                let path = git::get_worktree_path(&stage.id, &self.config.repo_root);
                if !path.is_dir() {
                    let reason = format!("its worktree {} is missing", path.display());
                    let err_msg = format!("Cannot hand stage '{}' on: {reason}", stage.id);
                    self.block_stranded_stage(&stage.id, err_msg);
                    return Ok(());
                }
                let branch = git::branch_name_for_stage(&stage.id);
                let mut worktree = Worktree::new(stage.id.clone(), path, branch);
                worktree.mark_active();
                worktree
            }
        };
        self.spawn_stage_agent(stage, worktree, None, kind)
    }
}

#[cfg(test)]
#[path = "contract_phase_tests.rs"]
mod tests;
