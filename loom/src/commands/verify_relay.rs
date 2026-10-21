//! `loom check` inside a session: the daemon runs the gate on the host and
//! the session only requests it and follows it. A session cannot run the
//! confined criteria itself (it cannot nest the sandbox), so a `Relay` session
//! is routed to `host_verify::wait`, a `Legacy` one is refused, and the
//! operator's shell keeps the local run. Every in-session failure prints the
//! `Host job: refused` first line and exits 1.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::host_verify::wait::{self, TicketSink, WaitOutcome, WaitSite};
use crate::host_verify::JobKind;
use crate::models::stage::{Stage, StageType};
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode};
use crate::relay::{HostJobRequest, RequestKind};
use crate::verify::transitions::load_stage;

const LEGACY_REFUSAL: &str = "This session cannot relay verification (no LOOM_SCRATCH_DIR). Run \
                              loom check <stage> from the operator shell.";

/// Route `loom check <stage_id>` by where it runs. Returns for the operator's
/// shell, which then runs locally; a session's outcome ends the process here.
/// `criteria` is the `--criterion` selection; empty requests every criterion.
pub(super) fn run(
    stage_id: &str,
    work_dir: &Path,
    suggest: bool,
    criteria: &[usize],
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
            let code = request_or_wait(&context, stage_id, work_dir, criteria, &mut out)?;
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

fn request_or_wait(
    context: &RelayContext,
    stage_id: &str,
    work_dir: &Path,
    criteria: &[usize],
    out: &mut dyn Write,
) -> Result<i32> {
    let target = match target(context, stage_id, work_dir) {
        Ok(target) => target,
        Err(error) => return refuse(out, &format!("{error:#}")),
    };
    let site = WaitSite {
        work_dir,
        stage_id,
        session_id: &context.session_id,
        checkout: &target.checkout,
        knowledge: target.knowledge,
        kind: JobKind::Check,
        no_cache: std::env::var("LOOM_ACCEPTANCE_CACHE").as_deref() == Ok("0"),
        // The selection rides on the ticket; the daemon's job runs only it.
        criteria: (!criteria.is_empty()).then_some(criteria),
        scratch_dir: &context.scratch_dir,
        budget: wait::budget_for(&target.stage),
    };
    let mut sink = TicketSink::default();
    let mut relay = |request: &HostJobRequest| -> Result<String> {
        let payload = serde_json::to_value(request)?;
        let line = context.emit(RequestKind::Check, payload, "loom check", false, &mut sink)?;
        Ok(line.id)
    };
    let outcome = wait::wait_or_request(&site, &mut relay, out)?;
    if outcome == WaitOutcome::Requested {
        sink.print_line(out)?;
    }
    out.flush().context("failed to flush loom check output")?;
    Ok(wait::exit_code(&outcome))
}

#[cfg(test)]
#[path = "verify_relay_tests.rs"]
mod tests;
