//! Fail-closed retirement for the manual stage reset command.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use crate::fs::session_files::{load_session_exact, mark_session_terminal_reason};
use crate::models::session::{Session, SessionExitReason, SessionStatus, SessionType};
use crate::orchestrator::session_registry::in_progress_sessions_for_stage;
use crate::orchestrator::terminal::backend::SessionBackend;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};
use crate::verify::transitions::{load_stage, update_stage};

use super::{apply_reset, live_agent_refusal, live_agents_for, LiveAgent};

const CONFIRM_ATTEMPTS: usize = 41;
const CONFIRM_INTERVAL: Duration = Duration::from_millis(50);

pub(super) trait ResetRuntime {
    fn kill(&self, work_dir: &Path, agents: &[LiveAgent]);
    fn identity_missing(&self, work_dir: &Path, agent: &LiveAgent) -> bool;
    fn is_alive(&self, work_dir: &Path, agent: &LiveAgent) -> Result<bool>;
    fn wait(&self);
}

pub(super) struct OsRuntime {
    backend: Option<SessionBackend>,
}

impl OsRuntime {
    pub(super) fn new(work_dir: &Path) -> Self {
        Self {
            backend: SessionBackend::from_config(work_dir.to_path_buf()).ok(),
        }
    }
}

impl ResetRuntime for OsRuntime {
    fn kill(&self, work_dir: &Path, agents: &[LiveAgent]) {
        super::kill_live_agents(work_dir, agents);
    }

    fn identity_missing(&self, work_dir: &Path, agent: &LiveAgent) -> bool {
        session_process_status(work_dir, &agent_session(agent)) == SessionProcessStatus::Missing
    }

    fn is_alive(&self, work_dir: &Path, agent: &LiveAgent) -> Result<bool> {
        let session = agent_session(agent);
        match &self.backend {
            Some(backend) => backend.is_session_alive(&session),
            None => Ok(matches!(
                session_process_status(work_dir, &session),
                SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Unverifiable
            )),
        }
    }

    fn wait(&self) {
        std::thread::sleep(CONFIRM_INTERVAL);
    }
}

pub(super) fn reset_with(
    work_dir: &Path,
    stage_id: &str,
    hard: bool,
    kill_session: bool,
    runtime: &impl ResetRuntime,
) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    if !kill_session {
        let live = live_agents_for(work_dir, stage_id)?;
        if !live.is_empty() {
            return Err(live_agent_refusal(stage_id, &live));
        }
    } else {
        let targets = retirement_targets(work_dir, stage_id, stage.session.as_deref())?;
        if targets.is_empty() {
            eprintln!("Note: Stage '{stage_id}' has no live agent to kill");
        } else {
            retire_targets(work_dir, stage_id, &targets, runtime)?;
        }
    }

    eprintln!(
        "Warning: Bypassing state machine to reset stage to initial state (was: {:?})",
        stage.status
    );
    update_stage(stage_id, work_dir, |current| {
        apply_reset(current);
        Ok(())
    })?;
    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

fn retirement_targets(
    work_dir: &Path,
    stage_id: &str,
    assigned_id: Option<&str>,
) -> Result<Vec<LiveAgent>> {
    let sessions = in_progress_sessions_for_stage(work_dir, stage_id)
        .with_context(|| format!("discovering sessions attached to stage '{stage_id}'"))?;
    let mut targets: Vec<LiveAgent> = sessions
        .into_iter()
        .filter(|session| session.session_type != SessionType::Adjudication)
        .map(LiveAgent::Known)
        .collect();
    include_assigned(work_dir, stage_id, assigned_id, &mut targets)?;
    let known: HashSet<String> = targets
        .iter()
        .map(|target| target.session_id().to_string())
        .collect();
    targets.extend(
        crate::orchestrator::session_registry::orphan_evidence(work_dir)
            .into_iter()
            .filter(|item| item.stage_id == stage_id && !known.contains(&item.session_id))
            .map(LiveAgent::Orphan),
    );
    Ok(targets)
}

fn include_assigned(
    work_dir: &Path,
    stage_id: &str,
    assigned_id: Option<&str>,
    targets: &mut Vec<LiveAgent>,
) -> Result<()> {
    let Some(id) = assigned_id else { return Ok(()) };
    if targets.iter().any(|target| target.session_id() == id) {
        return Ok(());
    }
    let assigned = load_session_exact(work_dir, id)?
        .with_context(|| format!("stage '{stage_id}' assigns missing session record '{id}'"))?;
    anyhow::ensure!(
        assigned.stage_id.as_deref() == Some(stage_id),
        "assigned session '{}' belongs to {:?}, not stage '{stage_id}'",
        assigned.id,
        assigned.stage_id
    );
    if !assigned.status.is_terminal() && assigned.session_type != SessionType::Adjudication {
        targets.push(LiveAgent::Known(assigned));
    }
    Ok(())
}

fn retire_targets(
    work_dir: &Path,
    stage_id: &str,
    targets: &[LiveAgent],
    runtime: &impl ResetRuntime,
) -> Result<()> {
    let missing: HashSet<String> = targets
        .iter()
        .filter(|target| runtime.identity_missing(work_dir, target))
        .map(|target| target.session_id().to_string())
        .collect();
    runtime.kill(work_dir, targets);
    confirm_all_gone(work_dir, stage_id, targets, &missing, runtime)?;
    for target in targets {
        if let LiveAgent::Known(session) = target {
            mark_session_terminal_reason(
                work_dir,
                &session.id,
                SessionStatus::ContextExhausted,
                SessionExitReason::OperatorStop,
            )
            .with_context(|| format!("recording operator stop for session '{}'", session.id))?;
        }
    }
    Ok(())
}

fn confirm_all_gone(
    work_dir: &Path,
    stage_id: &str,
    targets: &[LiveAgent],
    missing: &HashSet<String>,
    runtime: &impl ResetRuntime,
) -> Result<()> {
    let mut pending: Vec<&LiveAgent> = targets.iter().collect();
    let mut failures = Vec::new();
    for attempt in 0..CONFIRM_ATTEMPTS {
        pending.retain(|target| match runtime.is_alive(work_dir, target) {
            Ok(_) if missing.contains(target.session_id()) => false,
            Ok(alive) => alive,
            Err(error) => {
                failures.push(format!("{} (probe error: {error:#})", target.session_id()));
                false
            }
        });
        if pending.is_empty() || !failures.is_empty() {
            break;
        }
        if attempt + 1 < CONFIRM_ATTEMPTS {
            runtime.wait();
        }
    }
    failures.extend(
        missing
            .iter()
            .map(|id| format!("{id} (PID identity unknown)")),
    );
    failures.extend(
        pending
            .iter()
            .map(|target| format!("{} (still alive)", target.session_id())),
    );
    anyhow::ensure!(
        failures.is_empty(),
        "Stage '{stage_id}' reset refused because retirement was not confirmed for: {}",
        failures.join(", ")
    );
    Ok(())
}

fn agent_session(agent: &LiveAgent) -> Session {
    match agent {
        LiveAgent::Known(session) => session.clone(),
        LiveAgent::Orphan(evidence) => {
            let mut session = Session::new();
            session.id = evidence.session_id.clone();
            session.stage_id = Some(evidence.stage_id.clone());
            session.tracking_key = evidence.tracking_key.clone();
            session.session_type = evidence.session_type;
            session.backend = evidence.backend;
            session.pid = Some(evidence.pid);
            session
        }
    }
}
