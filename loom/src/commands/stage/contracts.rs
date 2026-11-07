//! `loom stage contracts {freeze,show,restore}`: the contract phase's freeze
//! (DESIGN D8) and the frozen record it leaves behind.
//!
//! This root holds what the three commands share: where a stage's contract
//! files live, and the daemon channel the freeze request travels over. The
//! channel mirrors `loom stage dispute-criteria`: the relay in a relayed
//! session (which then waits for the host job), else the socket, else the
//! worktree spool when this process may not dial the socket at all.

pub(crate) mod freeze;
mod freeze_follow;
mod freeze_pending;
mod restore;
mod show;

pub use freeze::freeze;
pub use restore::restore;
pub use show::show;

use std::io::Write;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::daemon::{
    current_session_id, try_send_request, user_credential, DaemonReach, Request, Response,
};
use crate::fs::stage_request::{append_to_spool, spool_path, spool_target_from_cwd, StageRequest};
use crate::host_verify::wait::{self, TicketSink, WaitOutcome, WaitSite};
use crate::host_verify::{JobKind, JobState};
use crate::plan::schema::ContractChoices;
use crate::relay::emit::{RelayContext, RelayMode, RelaySink};
use crate::relay::{HostJobRequest, RequestKind};
use crate::verify::contracts::refusal;
use crate::verify::contracts::site::ContractSite;
use crate::verify::transitions::load_stage;

use super::acceptance_runner::resolve_stage_execution_paths;

/// What an accepted socket or spool request prints: no session waits for it,
/// so the daemon does the rest. A relayed session waits and prints the job's
/// own lines instead.
const RUN_REQUESTED: &str = "Contract run requested. End your turn now: the daemon runs the \
     contracts on the host, then freezes them and starts the implementation session, or sends \
     the refusal to an adjudicator, whose verdict a fresh contract writer starts with.";

impl ContractSite {
    /// Load the stage the way the session's own commands do, from the
    /// state root the current directory names.
    fn load(stage_id: &str) -> Result<Self> {
        let work_dir = crate::commands::common::work_dir_path()?;
        let stage = load_stage(stage_id, &work_dir)?;
        let paths = resolve_stage_execution_paths(&stage)?;
        let (Some(worktree_root), Some(working_dir)) = (paths.worktree_root, paths.acceptance_dir)
        else {
            bail!("Stage '{stage_id}' has no worktree, so it has no contract files");
        };
        Ok(Self::resolved(work_dir, stage, worktree_root, working_dir))
    }
}

/// What one freeze asks the daemon to run: the stage, the HEAD and tree this
/// process saw, and the choices of this call only (the daemon merges them with
/// the ones it saved).
struct FreezeCall<'a> {
    stage_id: &'a str,
    site: &'a ContractSite,
    head: &'a str,
    tree_digest: &'a str,
    choices: &'a ContractChoices,
}

/// The ticket payload of the relayed freeze: the request the wait computed
/// for this checkout, plus the choices. The flag values never reach the relay
/// line; they travel in the ticket file.
fn freeze_payload(request: &HostJobRequest, choices: &ContractChoices) -> Result<Value> {
    serde_json::to_value(StageRequest::FreezeContracts {
        head: request.head.clone(),
        tree_digest: request.tree_digest.clone(),
        choices: choices.clone(),
    })
    .context("failed to serialize the freeze request")
}

/// Request the host contract run over the relay, the socket, or the spool.
/// A relayed session waits for the job like `loom check` does and follows the
/// daemon's application of it, then ends the process with the outcome's exit
/// code; the relay line is the last line of the call that sent the ticket.
fn send_freeze(
    call: &FreezeCall<'_>,
    relay_mode: RelayMode,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        let out = sink.stdout();
        let code = relay_freeze(&context, call, out)?;
        out.flush().context("failed to flush the freeze output")?;
        std::process::exit(code)
    }

    let request = StageRequest::FreezeContracts {
        head: call.head.to_string(),
        tree_digest: call.tree_digest.to_string(),
        choices: call.choices.clone(),
    };
    let work_dir = crate::commands::common::work_dir_path()?;
    let rpc = Request::FreezeContracts {
        auth_token: user_credential(&work_dir),
        stage_id: call.stage_id.to_string(),
        session_id: current_session_id(),
        head: call.head.to_string(),
        tree_digest: call.tree_digest.to_string(),
        choices: call.choices.clone(),
    };
    match try_send_request(&work_dir, &rpc)? {
        DaemonReach::Answered(response) => handle_freeze_response(call.stage_id, response),
        DaemonReach::NotListening => bail!(
            "No daemon is listening on the state directory's orchestrator.sock, so the contract \
             run cannot be requested. Nothing was frozen; run the command again once the daemon \
             is running."
        ),
        DaemonReach::Unreachable => queue_freeze(call.stage_id, &request),
    }
}

/// The relayed freeze: refuse a rerun under other flags than a pending
/// request's, then request the job or follow the one already requested
/// (`wait::wait_or_request`), and follow the daemon's application of a
/// finished job (`freeze_follow`). Returns the process exit code.
///
/// The wait reached the daemon (a ticket sent) or follows a job that did, so
/// the writer's recorded refusal is cleared here; a failed host run is the
/// job's own result and is never recorded as a CLI refusal.
fn relay_freeze(context: &RelayContext, call: &FreezeCall<'_>, out: &mut dyn Write) -> Result<i32> {
    let started = Instant::now();
    let site = WaitSite {
        work_dir: &call.site.work_dir,
        stage_id: call.stage_id,
        session_id: &context.session_id,
        checkout: &call.site.worktree_root,
        knowledge: false,
        kind: JobKind::Freeze,
        no_cache: false,
        criteria: None,
        scratch_dir: &context.scratch_dir,
        budget: wait::budget_for(&call.site.stage),
    };
    freeze_pending::refuse_other_flags(&site, call.choices)?;
    let mut ticket = TicketSink::default();
    let mut relay = |request: &HostJobRequest| -> Result<String> {
        let payload = freeze_payload(request, call.choices)?;
        // The session waits for the job, so the notice does not tell it to end its turn.
        let line = context.emit(
            RequestKind::FreezeContracts,
            payload,
            "stage contracts freeze",
            false,
            &mut ticket,
        )?;
        Ok(line.id)
    };
    let outcome = wait::wait_or_request(&site, &mut relay, out)?;
    // Best effort: a stale local refusal note must not strand a request already relayed.
    if let Err(error) = refusal::clear(&context.scratch_dir) {
        tracing::warn!(error = %format!("{error:#}"), "Cannot clear the local freeze refusal note");
    }
    match outcome {
        WaitOutcome::Requested { request_id } => {
            freeze_pending::record_sent(&site, &request_id, call.choices);
            ticket.publish_request(&site, &request_id, out)?;
            Ok(0)
        }
        WaitOutcome::Finished(record, _)
            if matches!(record.state, JobState::Passed | JobState::Failed) =>
        {
            let remaining = site.budget.saturating_sub(started.elapsed());
            freeze_follow::follow_application(&site, &record, remaining, out).or_else(|error| {
                wait::refusal(out, JobKind::Freeze, &record.head, &format!("{error:#}"))
            })
        }
        other => Ok(wait::exit_code(&other)),
    }
}

/// Queue the request for the daemon, for the caller that cannot reach it.
/// The daemon runs the same handler on it, so the session and path checks
/// still apply, and attributes it to the worktree it drained it from.
fn queue_freeze(stage_id: &str, request: &StageRequest) -> Result<()> {
    let worktree_root = spool_target_from_cwd()?;
    append_to_spool(&worktree_root, request)?;

    println!("Queued the contract run of stage '{stage_id}' for the loom daemon.");
    println!("Queued at: {}", spool_path(&worktree_root).display());
    println!("{RUN_REQUESTED}");
    Ok(())
}

/// A live daemon's refusal is authoritative and reported verbatim.
fn handle_freeze_response(stage_id: &str, response: Response) -> Result<()> {
    match response {
        Response::Ok => {
            println!("{RUN_REQUESTED}");
            Ok(())
        }
        Response::Error { message } => bail!("Daemon refused the freeze: {message}"),
        Response::AuthenticationFailed => bail!(
            "Daemon refused the freeze: it accepted no credential and could not confirm this \
             process is running inside the contract session of stage '{stage_id}'"
        ),
        other => bail!("Unexpected daemon response to FreezeContracts: {other:?}"),
    }
}
