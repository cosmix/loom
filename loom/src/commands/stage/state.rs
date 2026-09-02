//! Stage state transition commands

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::daemon::{
    current_session_id, try_send_request, user_credential, DaemonReach, Request, Response,
};
use crate::fs::stage_request::{append_to_spool, spool_path, spool_target_from_cwd, StageRequest};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::session_registry::{
    live_sessions_for_stage, orphan_evidence, OrphanEvidence,
};
use crate::orchestrator::terminal::backend::SessionBackend;
use crate::orchestrator::terminal::native;
use crate::verify::transitions::{load_stage, update_stage};

/// Block a stage with a reason.
///
/// Routed through the daemon whenever one is listening, because the state
/// directory is read-only from a stage worktree: the caller that most needs to
/// record why it is stuck is exactly the one that cannot write `stages/<id>.md`
/// itself.
/// With no daemon reachable there is nothing to route to, and an operator's
/// direct write is the only path — the same one this command always took.
///
/// A daemon refusal is an ANSWER, and is reported as one. Falling back to the
/// direct write on refusal would hand an agent precisely the write the sandbox
/// denies it. A stale socket left behind by a daemon that died uncleanly is
/// NOT an answer — there is no authority behind it to defer to — so it takes
/// the direct-write path exactly like no socket at all.
///
/// A sandboxed stage agent reaches neither arm: its socket syscalls are denied
/// before the path is consulted, so it cannot tell whether a daemon is there
/// and must not assume it isn't. That case queues the block for the daemon
/// instead — see `queue_block_request`.
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let request = Request::BlockStage {
        auth_token: user_credential(&work_dir),
        stage_id: stage_id.clone(),
        session_id: current_session_id(),
        reason: reason.clone(),
    };

    match try_send_request(&work_dir, &request)? {
        DaemonReach::Answered(response) => handle_block_response(&stage_id, response)?,
        DaemonReach::NotListening => {
            update_stage(&stage_id, &work_dir, |stage| {
                stage.try_mark_blocked()?;
                stage.close_reason = Some(reason.clone());
                stage.updated_at = chrono::Utc::now();
                Ok(())
            })?;
        }
        DaemonReach::Unreachable => return queue_block_request(&stage_id, &reason),
    }

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Queue a block for the daemon to apply, for the caller that cannot reach it.
///
/// This is the sandboxed stage agent's path, and the only one it has: its
/// state directory's `stages/` write is denied and so are its socket syscalls. Queueing
/// does not weaken the authorization the RPC path establishes — the daemon
/// still decides whether the stage may be blocked, and still attributes the
/// request to the worktree it drained it from, never to anything the request
/// claims about itself.
fn queue_block_request(stage_id: &str, reason: &str) -> Result<()> {
    let worktree_root = spool_target_from_cwd()?;
    append_to_spool(
        &worktree_root,
        &StageRequest::Block {
            reason: reason.to_string(),
        },
    )?;

    println!("Queued a block of stage '{stage_id}' for the loom daemon to apply.");
    println!("Reason: {reason}");
    println!("Queued at: {}", spool_path(&worktree_root).display());
    println!(
        "The daemon applies it on its next poll; run `loom status` to confirm the stage \
         reaches Blocked."
    );
    Ok(())
}

/// Interpret the daemon's answer to a `BlockStage` request. A live daemon's
/// refusal is authoritative and reported verbatim — never a reason to fall
/// back to the direct write.
fn handle_block_response(stage_id: &str, response: Response) -> Result<()> {
    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => {
            bail!("Daemon refused to block stage '{stage_id}': {message}")
        }
        Response::AuthenticationFailed => bail!(
            "Daemon refused to block stage '{stage_id}': it accepted no credential and could \
             not confirm this process is running inside the session that owns the stage. \
             Check that the loom daemon is running and that this is that session"
        ),
        other => bail!("Unexpected daemon response to BlockStage: {other:?}"),
    }
}

/// A live agent process found for a stage: either a session the registry can
/// account for, or an orphan seen only through PID evidence because its
/// stage link went missing (the hazard `orphan_evidence` exists to catch).
enum LiveAgent {
    Known(Session),
    Orphan(OrphanEvidence),
}

impl LiveAgent {
    fn session_id(&self) -> &str {
        match self {
            LiveAgent::Known(session) => &session.id,
            LiveAgent::Orphan(evidence) => &evidence.session_id,
        }
    }

    /// Human-readable description for the refusal message: names the
    /// session, its pid (if known), and its backend. Never claims a pid we
    /// have not actually observed.
    fn describe(&self) -> String {
        match self {
            LiveAgent::Known(session) => {
                let pid = session
                    .pid
                    .map_or_else(|| "unknown".to_string(), |pid| pid.to_string());
                format!(
                    "session '{}' (pid {pid}, {} backend)",
                    session.id, session.backend
                )
            }
            LiveAgent::Orphan(evidence) => format!(
                "orphan session '{}' (pid {}, {} backend, no session record)",
                evidence.session_id, evidence.pid, evidence.backend
            ),
        }
    }
}

/// Every live agent process associated with `stage_id`: sessions the
/// registry can account for, plus orphans it can only see via PID evidence
/// (e.g. after a daemon crash severed `stage.session`).
fn live_agents_for(work_dir: &Path, stage_id: &str) -> Result<Vec<LiveAgent>> {
    let mut agents: Vec<LiveAgent> = live_sessions_for_stage(work_dir, stage_id)?
        .into_iter()
        // The judge is not an agent working the stage: it must neither block
        // a reset nor be killed by one, and this is also what lets an
        // adjudicator run the reset it diagnoses. Orphan evidence never
        // carries adjudication sessions (`SESSION_KINDS` omits it), so only
        // this `Known` arm needs the filter.
        .filter(|session| session.session_type != crate::models::session::SessionType::Adjudication)
        .map(LiveAgent::Known)
        .collect();
    agents.extend(
        orphan_evidence(work_dir)
            .into_iter()
            .filter(|evidence| evidence.stage_id == stage_id)
            .map(LiveAgent::Orphan),
    );
    Ok(agents)
}

/// Build the refusal error for a reset blocked by live agents: names what is
/// running and both ways forward.
fn live_agent_refusal(stage_id: &str, agents: &[LiveAgent]) -> anyhow::Error {
    let details = agents
        .iter()
        .map(LiveAgent::describe)
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::anyhow!(
        "Stage '{stage_id}' still has a live agent running ({details}); refusing to reset. \
         Run 'loom stage reset {stage_id} --kill-session' to terminate it first, or leave it \
         alone: the daemon adopts a live session instead of spawning a duplicate."
    )
}

/// Kill every live agent found for the stage. Known sessions go through the
/// configured backend; orphans have no session record for a backend to
/// dispatch on, so they go through the shared PID-identity teardown
/// directly, reconstructing just enough of a `Session` to identify them.
fn kill_live_agents(work_dir: &Path, agents: &[LiveAgent]) {
    for agent in agents {
        let result = match agent {
            LiveAgent::Known(session) => kill_known_session(work_dir, session),
            LiveAgent::Orphan(evidence) => kill_orphan_session(work_dir, evidence),
        };
        if let Err(e) = result {
            eprintln!(
                "Warning: Failed to kill session '{}': {e}",
                agent.session_id()
            );
        }
    }
}

fn kill_known_session(work_dir: &Path, session: &Session) -> Result<()> {
    let backend = SessionBackend::from_config(work_dir.to_path_buf())
        .context("Failed to construct session backend")?;
    if backend.is_session_alive(session)? {
        backend.kill_session(session)?;
        println!("  Killed session '{}'", session.id);
    } else {
        println!("  Session '{}' already terminated", session.id);
    }
    Ok(())
}

/// Terminate an orphan through the same PID-identity path the backend falls
/// back to when it has no window to close, since there is no session record
/// here for a backend to be constructed against.
fn kill_orphan_session(work_dir: &Path, evidence: &OrphanEvidence) -> Result<()> {
    let mut session = Session::new();
    session.id = evidence.session_id.clone();
    session.stage_id = Some(evidence.stage_id.clone());
    session.tracking_key = evidence.tracking_key.clone();
    session.session_type = evidence.session_type;
    session.backend = evidence.backend;
    session.pid = Some(evidence.pid);

    native::pid_only_terminate(work_dir, &session)?;
    println!("  Killed orphan session '{}'", evidence.session_id);
    Ok(())
}

/// Reset a stage to pending
///
/// NOTE: This is a manual recovery command that intentionally bypasses state machine validation.
/// WaitingForDeps has no incoming transitions because it's the initial state. For recovery scenarios,
/// we allow direct assignment to reset stages to their initial state.
pub fn reset(stage_id: String, hard: bool, kill_session: bool) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    let stage = load_stage(&stage_id, &work_dir)?;

    // Refuse to reset while an agent is still running for this stage, unless
    // told to kill it first. This prevents a duplicate-session hazard where
    // the old session keeps running while the respawned stage starts a new
    // one. Checks both tracked sessions and orphan PID evidence: a stage
    // whose `session` link went missing (e.g. a daemon crash) is exactly the
    // case a `stage.session`-only check misses.
    let live_agents = live_agents_for(&work_dir, &stage_id)?;
    if !live_agents.is_empty() {
        if kill_session {
            kill_live_agents(&work_dir, &live_agents);
        } else {
            return Err(live_agent_refusal(&stage_id, &live_agents));
        }
    } else if kill_session {
        eprintln!("Note: Stage '{stage_id}' has no live agent to kill");
    }

    // INTENTIONAL STATE MACHINE BYPASS: WaitingForDeps is the initial state and
    // has no valid incoming transitions. Apply only reset-owned fields to the
    // fresh record under lock so unrelated concurrent changes survive.
    eprintln!(
        "Warning: Bypassing state machine to reset stage to initial state (was: {:?})",
        stage.status
    );
    update_stage(&stage_id, &work_dir, |current| {
        apply_reset(current);
        Ok(())
    })?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    let mut skipped_status = None;
    update_stage(&stage_id, &work_dir, |stage| {
        if stage.status != StageStatus::Executing {
            skipped_status = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_waiting_for_input()
    })?;
    if let Some(status) = skipped_status {
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, status
        );
        return Ok(());
    }

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    let mut skipped_status = None;
    update_stage(&stage_id, &work_dir, |stage| {
        if stage.status != StageStatus::WaitingForInput {
            skipped_status = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_executing()
    })?;
    if let Some(status) = skipped_status {
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, status
        );
        return Ok(());
    }

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}

/// Hold a stage (prevent auto-execution even when ready)
pub fn hold(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    let mut already_held = false;
    update_stage(&stage_id, &work_dir, |stage| {
        if stage.held {
            already_held = true;
        } else {
            stage.hold();
        }
        Ok(())
    })?;
    if already_held {
        println!("Stage '{stage_id}' is already held");
        return Ok(());
    }

    println!("Stage '{stage_id}' held");
    println!("The stage will not auto-execute. Use 'loom stage release {stage_id}' to unlock.");
    Ok(())
}

/// Release a held stage (allow auto-execution)
pub fn release(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;

    let mut already_released = false;
    update_stage(&stage_id, &work_dir, |stage| {
        if !stage.held {
            already_released = true;
        } else {
            stage.release();
        }
        Ok(())
    })?;
    if already_released {
        println!("Stage '{stage_id}' is not held");
        return Ok(());
    }

    println!("Stage '{stage_id}' released");
    Ok(())
}

fn apply_reset(stage: &mut Stage) {
    stage.status = StageStatus::WaitingForDeps;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.started_at = None;
    stage.duration_secs = None;
    stage.retry_count = 0;
    stage.fix_attempts = 0;
    stage.last_failure_at = None;
    stage.failure_info = None;
    // Cleared in both soft and hard resets: by this point any live agent has
    // either been killed or refused (see `reset`), so a stage naming a
    // session it no longer owns is the exact inconsistency this fix exists
    // to eliminate.
    stage.session = None;
    stage.updated_at = chrono::Utc::now();
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod state_tests;
