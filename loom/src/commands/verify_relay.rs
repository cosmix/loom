//! `loom check` inside a session: the daemon runs the gate on the host and
//! the session only requests it and follows it. A session cannot run the
//! confined criteria itself (it cannot nest the sandbox), so a `Relay` session
//! is routed to `host_verify::wait`, a `Legacy` one is refused, and the
//! operator's shell keeps the local run. Every in-session failure prints the
//! `Host job: refused` first line and exits 1.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{timing, CheckSelection};
use crate::commands::stage::read_replacements;
use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::host_verify::wait::{self, TicketSink, WaitOutcome, WaitSite};
use crate::host_verify::JobKind;
use crate::models::dispute::FileReplacement;
use crate::models::stage::{Stage, StageType};
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode};
use crate::relay::{HostJobRequest, RequestKind};
use crate::verify::transitions::load_stage;

const LEGACY_REFUSAL: &str = "This session cannot relay verification (no LOOM_SCRATCH_DIR). Run \
                              loom check <stage> from the operator shell.";

/// Route `loom check <stage_id>` by where it runs. Returns for the operator's
/// shell, which then runs locally; a session's outcome ends the process here.
/// `selection` is what the call runs; the daemon's job runs only it. `force`
/// overrides the timing refusals of a standard stage (`verify_timing.rs`).
pub(super) fn run(
    stage_id: &str,
    work_dir: &Path,
    suggest: bool,
    selection: &CheckSelection,
    force: bool,
) -> Result<()> {
    let mut out = std::io::stdout();
    match mode(&EnvSnapshot::from_process_env()) {
        RelayMode::Operator => Ok(()),
        RelayMode::Legacy => {
            let code = refuse(&mut out, LEGACY_REFUSAL)?;
            std::process::exit(code)
        }
        RelayMode::Relay(context) => {
            if suggest {
                eprintln!("--suggest needs a local run; ignored in a session");
            }
            // Every session outcome ends here: requested and pending exit 0
            // too, and must never fall through to the local run.
            let code = request_or_wait(&context, stage_id, work_dir, (selection, force), &mut out)?;
            let _ = out.flush();
            std::process::exit(code)
        }
    }
}

/// The refusal's first line names the session's own checkout, which is the
/// current directory: the stage's checkout may be what failed to resolve.
fn refuse(out: &mut dyn Write, message: &str) -> Result<i32> {
    let head = std::env::current_dir()
        .map(|cwd| wait::head_or_empty(&cwd))
        .unwrap_or_default();
    wait::refusal(out, JobKind::Check, &head, message)
}

/// The stage and the checkout its gate runs on: the stage worktree, or the
/// repository root for a knowledge stage.
struct Target {
    stage: Stage,
    checkout: PathBuf,
    knowledge: bool,
}

/// Everything the relay guards require, then the stage and its checkout.
fn target(context: &RelayContext, stage_id: &str, work_dir: &Path) -> Result<Target> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::Check, Some(stage_id), &cwd, uid)?;
    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;
    let repo_root = WorkDir::new(work_dir)?
        .main_project_root()
        .context("the repository root is unavailable")?;
    let knowledge = stage.stage_type == StageType::Knowledge;
    let checkout = if knowledge {
        repo_root
    } else {
        stage_worktree_path(&stage, &repo_root)?
    };
    Ok(Target {
        stage,
        checkout,
        knowledge,
    })
}

/// The replacement files a trial lays over frozen files, read under the
/// rules a contract dispute's replacements follow; none for any other run.
fn replacements(
    work_dir: &Path,
    stage_id: &str,
    selection: &CheckSelection,
) -> Result<Vec<FileReplacement>> {
    match selection {
        CheckSelection::Trial(files) => read_replacements(work_dir, stage_id, files),
        _ => Ok(Vec::new()),
    }
}

fn selected_criteria(selection: &CheckSelection) -> Option<&[usize]> {
    match selection {
        CheckSelection::Criteria(criteria) | CheckSelection::AtBase(criteria) => {
            Some(criteria.as_slice())
        }
        _ => None,
    }
}

fn runs_contracts(selection: &CheckSelection) -> bool {
    matches!(
        selection,
        CheckSelection::Contracts | CheckSelection::Trial(_)
    )
}

/// The wait this call requests or follows: the selection rides on the ticket,
/// and the daemon's job runs only it.
fn wait_site<'a>(
    context: &'a RelayContext,
    (stage_id, work_dir): (&'a str, &'a Path),
    target: &'a Target,
    selection: &'a CheckSelection,
    replacements: &'a [FileReplacement],
) -> WaitSite<'a> {
    WaitSite {
        work_dir,
        stage_id,
        session_id: &context.session_id,
        checkout: &target.checkout,
        knowledge: target.knowledge,
        kind: JobKind::Check,
        no_cache: std::env::var("LOOM_ACCEPTANCE_CACHE").as_deref() == Ok("0"),
        criteria: selected_criteria(selection),
        contracts: runs_contracts(selection).then_some(replacements),
        at_base: matches!(selection, CheckSelection::AtBase(_)),
        scratch_dir: &context.scratch_dir,
        budget: wait::budget_for(&target.stage),
    }
}

fn request_or_wait(
    context: &RelayContext,
    stage_id: &str,
    work_dir: &Path,
    (selection, force): (&CheckSelection, bool),
    out: &mut dyn Write,
) -> Result<i32> {
    let found = target(context, stage_id, work_dir)
        .and_then(|target| Ok((target, replacements(work_dir, stage_id, selection)?)));
    let (target, replacements) = match found {
        Ok(found) => found,
        Err(error) => return refuse(out, &format!("{error:#}")),
    };
    let site = wait_site(
        context,
        (stage_id, work_dir),
        &target,
        selection,
        &replacements,
    );
    // A call that finds its request state is the wait that collects the
    // result; a state that cannot be read surfaces as `wait_or_request`'s
    // refusal.
    let joins_pending = || !matches!(wait::pending_request_id(&site), Ok(None));
    let args = (selection, force);
    if let Some(text) = timing::refusal(context, &target.stage, work_dir, args, joins_pending) {
        eprintln!("{text}");
        return Ok(timing::REFUSED_EXIT);
    }
    let mut sink = TicketSink::default();
    let mut relay = |request: &HostJobRequest| -> Result<String> {
        let payload = serde_json::to_value(request)?;
        let line = context.emit(RequestKind::Check, payload, "loom check", false, &mut sink)?;
        Ok(line.id)
    };
    let outcome = wait::wait_or_request(&site, &mut relay, out)?;
    if let WaitOutcome::Requested { request_id } = &outcome {
        sink.publish_request(&site, request_id, out)?;
    }
    if let WaitOutcome::Finished(record, result) = &outcome {
        let outcomes = result.as_ref().map_or(&[][..], |result| &result.criteria);
        let who = (context, &target.stage);
        if let Some(line) = timing::next_step(who, selection, record.state, outcomes) {
            writeln!(out, "{line}")?;
        }
    }
    out.flush().context("failed to flush loom check output")?;
    Ok(wait::exit_code(&outcome))
}

#[cfg(test)]
#[path = "verify_relay_tests.rs"]
mod tests;
