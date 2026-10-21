//! `loom check` inside a session: the daemon runs the gate on the host and
//! the session only requests it and follows it. A session cannot run the
//! confined criteria itself (it cannot nest the sandbox), so a `Relay` session
//! is routed to `host_verify::wait`, a `Legacy` one is refused, and the
//! operator's shell keeps the local run.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::host_verify::wait::{self, WaitSite};
use crate::host_verify::JobKind;
use crate::models::stage::StageType;
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, StdSink};
use crate::relay::RequestKind;
use crate::verify::transitions::load_stage;

/// Route `loom check <stage_id>` by where it runs. Returns for the operator's
/// shell, which then runs locally; a session's outcome ends the process here.
pub(super) fn run(stage_id: &str, work_dir: &Path, suggest: bool) -> Result<()> {
    match mode(&EnvSnapshot::from_process_env()) {
        RelayMode::Operator => Ok(()),
        RelayMode::Legacy => refuse_legacy(),
        RelayMode::Relay(context) => {
            if suggest {
                eprintln!("--suggest needs a local run; ignored in a session");
            }
            // Every session outcome ends here: requested and pending exit 0
            // too, and must never fall through to the local run.
            let code = request_or_wait(&context, stage_id, work_dir)?;
            let _ = std::io::stdout().flush();
            std::process::exit(code)
        }
    }
}

fn refuse_legacy() -> Result<()> {
    let head = std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::host_verify::digest::head(&cwd).ok())
        .unwrap_or_default();
    let short: String = head.chars().take(12).collect();
    println!("Host job: refused (check, {short})");
    println!(
        "This session cannot relay verification (no LOOM_SCRATCH_DIR). Run loom check <stage> \
         from the operator shell."
    );
    std::process::exit(1)
}

fn request_or_wait(context: &RelayContext, stage_id: &str, work_dir: &Path) -> Result<i32> {
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
        repo_root.clone()
    } else {
        stage_worktree_path(&stage, &repo_root)?
    };
    let site = WaitSite {
        work_dir,
        stage_id,
        session_id: &context.session_id,
        checkout: &checkout,
        knowledge,
        kind: JobKind::Check,
        no_cache: std::env::var("LOOM_ACCEPTANCE_CACHE").as_deref() == Ok("0"),
        scratch_dir: &context.scratch_dir,
        budget: wait::budget_for(&stage),
    };
    let mut sink = StdSink::default();
    let mut relay = |request: &crate::relay::HostJobRequest| -> Result<String> {
        let line = context.emit(
            RequestKind::Check,
            serde_json::to_value(request)?,
            "loom check",
            false,
            &mut sink,
        )?;
        Ok(line.id)
    };
    let mut out = std::io::stdout();
    let outcome = wait::wait_or_request(&site, &mut relay, &mut out)?;
    out.flush().context("failed to flush loom check output")?;
    Ok(wait::exit_code(&outcome))
}
