//! Operator adapter for the shared contract freeze service.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;

use crate::git::worktree::WorktreeGit;
use crate::host_verify::wait::{head_or_empty, refusal as host_refusal};
use crate::host_verify::{digest, JobKind};
use crate::relay::emit::{mode, EnvSnapshot, RelayMode, RelaySink, StdSink};
use crate::relay::RequestKind;
use crate::verify::contracts::freeze::{check_site_changes, listed_problems};
use crate::verify::contracts::refusal;

#[cfg(test)]
use crate::models::stage::CommandConfinement;
#[cfg(test)]
use crate::plan::schema::ContractSpec;
#[cfg(test)]
use crate::testrun::{classify, RunOutcome};
#[cfg(test)]
use crate::testrun::{RunSummary, TestRunnerAdapter};
#[cfg(test)]
use crate::verify::contracts::collection::ScaffoldCheck;
#[cfg(test)]
use crate::verify::contracts::freeze::UNVERIFIED;
#[cfg(test)]
use crate::verify::contracts::freeze::{
    check_changes, check_formatting, judge_run, refuse, FreezeInputs,
};
#[cfg(test)]
mod prepared;
#[cfg(test)]
use prepared::{prepare_contract, run_prepared};

use super::{send_freeze, ContractSite};

/// `loom stage contracts freeze <stage-id>`. A failure in a session prints the
/// `Host job: refused` first line and exits 1, as `loom check` does.
pub fn freeze(stage_id: String) -> Result<()> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let in_session = !matches!(relay_mode, RelayMode::Operator);
    match run_freeze(&stage_id, relay_mode) {
        Err(error) if in_session => {
            let code = session_refusal(&mut std::io::stdout(), &error)?;
            std::process::exit(code)
        }
        outcome => outcome,
    }
}

/// The refusal a failed in-session freeze prints; its first line names HEAD of
/// the session's own checkout, the current directory.
fn session_refusal(out: &mut dyn Write, error: &anyhow::Error) -> Result<i32> {
    let head = std::env::current_dir()
        .map(|cwd| head_or_empty(&cwd))
        .unwrap_or_default();
    host_refusal(out, JobKind::Freeze, &head, &format!("{error:#}"))
}

fn run_freeze(stage_id: &str, relay_mode: RelayMode) -> Result<()> {
    let scratch_dir = match &relay_mode {
        RelayMode::Relay(context) => {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            // SAFETY: `getuid` has no preconditions and cannot fail.
            let uid = unsafe { libc::getuid() };
            context.check(RequestKind::FreezeContracts, Some(stage_id), &cwd, uid)?;
            Some(context.scratch_dir.clone())
        }
        RelayMode::Legacy | RelayMode::Operator => None,
    };
    let outcome = request_run(stage_id, relay_mode, &mut StdSink::default());
    if let Some(scratch_dir) = scratch_dir {
        keep_outcome(&scratch_dir, &outcome);
    }
    outcome
}

/// D8 steps 1 and 2 where the contract session runs, then the request for
/// the host run of the HEAD and tree this process sees. Git runs as the
/// worktree's `.git` directs it: this is the session's own process, and the
/// daemon checks the changes again with git pinned to the stage's registered
/// git directory, before the run and before it freezes anything.
fn request_run(stage_id: &str, relay_mode: RelayMode, sink: &mut dyn RelaySink) -> Result<()> {
    let site = ContractSite::load(stage_id)?;
    check_site_changes(&site, &WorktreeGit::discovered(&site.worktree_root))?;
    if matches!(relay_mode, RelayMode::Legacy) {
        // A session that cannot relay would have to run the contracts itself,
        // which would nest the sandbox: refuse, as the session's completion does.
        bail!(
            "This session cannot relay the contract freeze (no LOOM_SCRATCH_DIR). Run loom stage \
             contracts freeze {stage_id} from the operator shell, or loom stage retry {stage_id} \
             for a session that can."
        );
    }
    let head = digest::head(&site.worktree_root)?;
    let tree_digest = digest::tree_digest(&site.worktree_root)?;
    send_freeze(stage_id, &head, &tree_digest, relay_mode, sink)
}

/// Leave this attempt's outcome where the Stop hook looks once the writer
/// stops: a failure stays on record until an attempt reaches the daemon.
/// Best-effort; the attempt's own result is what the caller reports.
fn keep_outcome(scratch_dir: &Path, outcome: &Result<()>) {
    let kept = match outcome {
        Ok(()) => refusal::clear(scratch_dir),
        Err(error) => refusal::record(scratch_dir, &listed_problems(error)),
    };
    if let Err(error) = kept {
        eprintln!("warning: the operator cannot be shown this freeze's outcome: {error:#}");
    }
}

#[cfg(test)]
#[path = "freeze_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "freeze_run_dir_tests.rs"]
mod run_dir_tests;

#[cfg(test)]
#[path = "freeze_session_tests.rs"]
mod session_tests;
