//! Operator adapter for the shared contract freeze service.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;

use crate::git::worktree::WorktreeGit;
use crate::host_verify::wait::{head_or_empty, refusal as host_refusal};
use crate::host_verify::{digest, JobKind};
use crate::plan::schema::{ContractChoices, ContractSpec};
use crate::relay::emit::{mode, EnvSnapshot, RelayMode, RelaySink, StdSink};
use crate::relay::RequestKind;
use crate::verify::contracts::changes::stage_base;
use crate::verify::contracts::choices;
use crate::verify::contracts::freeze::{check_site_changes, listed_problems};
use crate::verify::contracts::refusal;

#[cfg(test)]
use crate::models::stage::CommandConfinement;
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

use super::{send_freeze, ContractSite, FreezeCall};

/// The `--test`, `--runner` and `--cwd` values of one call, as typed.
struct Flags {
    test: Vec<String>,
    runner: Vec<String>,
    cwd: Vec<String>,
}

/// `loom stage contracts freeze <stage-id> [--test ID=T] [--runner ID=A]
/// [--cwd ID=D]`. A failure in a session prints the `Host job: refused` first
/// line and exits 1, as `loom check` does.
pub fn freeze(
    stage_id: String,
    test: Vec<String>,
    runner: Vec<String>,
    cwd: Vec<String>,
) -> Result<()> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let in_session = !matches!(relay_mode, RelayMode::Operator);
    let flags = Flags { test, runner, cwd };
    match run_freeze(&stage_id, relay_mode, &flags) {
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

fn run_freeze(stage_id: &str, relay_mode: RelayMode, flags: &Flags) -> Result<()> {
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
    let outcome = request_run(stage_id, relay_mode, flags, &mut StdSink::default());
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
fn request_run(
    stage_id: &str,
    relay_mode: RelayMode,
    flags: &Flags,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let mut site = ContractSite::load(stage_id)?;
    let parsed = choices::parse_choices(&flags.test, &flags.runner, &flags.cwd)?;
    site.stage.contracts = checked_overlay(&site, &parsed)?;
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
    let call = FreezeCall {
        stage_id,
        site: &site,
        head: &head,
        tree_digest: &tree_digest,
        choices: &parsed,
    };
    send_freeze(&call, relay_mode, sink)
}

/// The refusals the daemon would record at admission, found here first: a
/// daemon admission refusal is a standing refusal the tick files at once, so
/// every writer mistake the CLI can catch stops before anything is relayed.
///
/// Loads the choices the daemon saved, drops the stale ones, merges this
/// call's flags and validates the result against the stage's base commit with
/// the writer's own git. Returns the plan's contracts overlaid with the saved
/// choices and this call's flags, so the scaffold check sees the tests the
/// writer named now and in earlier freezes. Only `parsed` is ever relayed; the
/// daemon merges it with the saved ones.
fn checked_overlay(site: &ContractSite, parsed: &ContractChoices) -> Result<Vec<ContractSpec>> {
    let plan_specs = &site.stage.contracts;
    let stage_id = &site.stage.id;
    let saved = choices::load_choices(&site.work_dir, stage_id)?;
    let merged = choices::merge_choices(&choices::prune_choices(plan_specs, &saved), parsed);
    let repo = WorktreeGit::discovered(&site.worktree_root);
    let base = stage_base(&repo, &site.work_dir)?;
    let (root, dir) = (&site.worktree_root, &site.working_dir);
    choices::validate_choices(plan_specs, &merged, root, dir, &base)?;
    choices::overlay_with_saved(&site.work_dir, stage_id, plan_specs, parsed)
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

#[cfg(test)]
#[path = "freeze_choices_tests.rs"]
mod choices_tests;
