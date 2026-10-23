//! `loom stage contracts {freeze,show,restore}`: the contract phase's freeze
//! (DESIGN D8) and the frozen record it leaves behind.
//!
//! This root holds what the three commands share: where a stage's contract
//! files live, and the daemon channel the freeze request travels over. The
//! channel mirrors `loom stage dispute-criteria`: the relay in a relayed
//! session, else the socket, else the worktree spool when this process may
//! not dial the socket at all.

pub(crate) mod freeze;
mod restore;
mod show;

pub use freeze::freeze;
pub use restore::restore;
pub use show::show;

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

use crate::daemon::{
    current_session_id, try_send_request, user_credential, DaemonReach, Request, Response,
};
use crate::fs::stage_request::{append_to_spool, spool_path, spool_target_from_cwd, StageRequest};
use crate::host_verify::wait::TicketSink;
use crate::models::stage::Stage;
use crate::relay::emit::{RelayMode, RelaySink};
use crate::relay::RequestKind;
use crate::verify::transitions::load_stage;

use super::acceptance_runner::resolve_stage_execution_paths;

/// What every accepted request prints: the daemon does the rest.
const RUN_REQUESTED: &str = "Contract run requested. End your turn now: the daemon runs the \
     contracts on the host, then freezes them and starts the implementation session, or sends \
     the refusal to an adjudicator, whose verdict a fresh contract writer starts with.";

/// A stage and the places its contract files live.
pub(crate) struct ContractSite {
    work_dir: PathBuf,
    stage: Stage,
    worktree_root: PathBuf,
    /// The stage's `working_dir` inside the worktree. Contract files, harness
    /// globs and frozen paths are all relative to it.
    working_dir: PathBuf,
}

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
        Ok(Self {
            work_dir,
            stage,
            worktree_root,
            working_dir,
        })
    }

    /// A site the caller has already resolved: the daemon's host run, which
    /// resolves it from the state root and never from its current directory.
    pub(crate) fn resolved(
        work_dir: PathBuf,
        stage: Stage,
        worktree_root: PathBuf,
        working_dir: PathBuf,
    ) -> Self {
        Self {
            work_dir,
            stage,
            worktree_root,
            working_dir,
        }
    }

    /// `working_dir` relative to the worktree root; empty when they coincide.
    fn working_dir_prefix(&self) -> PathBuf {
        let declared = self.stage.working_dir.as_deref().unwrap_or(".");
        Path::new(declared)
            .components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .collect()
    }
}

/// Request the host contract run for `head` and `tree_digest` over the
/// relay, the socket, or the spool. The relay line is the last line printed.
fn send_freeze(
    stage_id: &str,
    head: &str,
    tree_digest: &str,
    relay_mode: RelayMode,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let request = StageRequest::FreezeContracts {
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
    };
    if let RelayMode::Relay(context) = relay_mode {
        let payload =
            serde_json::to_value(&request).context("failed to serialize the freeze request")?;
        // The ticket is held back so a request that could not be written never
        // claims it was made, and the relay line still comes last.
        let mut ticket = TicketSink::default();
        context.emit(
            RequestKind::FreezeContracts,
            payload,
            "stage contracts freeze",
            true,
            &mut ticket,
        )?;
        println!("{RUN_REQUESTED}");
        ticket.print_line(sink.stdout())?;
        return Ok(());
    }

    let work_dir = crate::commands::common::work_dir_path()?;
    let rpc = Request::FreezeContracts {
        auth_token: user_credential(&work_dir),
        stage_id: stage_id.to_string(),
        session_id: current_session_id(),
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
    };
    match try_send_request(&work_dir, &rpc)? {
        DaemonReach::Answered(response) => handle_freeze_response(stage_id, response),
        DaemonReach::NotListening => bail!(
            "No daemon is listening on the state directory's orchestrator.sock, so the contract \
             run cannot be requested. Nothing was frozen; run the command again once the daemon \
             is running."
        ),
        DaemonReach::Unreachable => queue_freeze(stage_id, &request),
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
