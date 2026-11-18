//! Host-observed progress of an authorized direct Codex worker.
//!
//! A direct forward leaves no companion job, and everything inside the stage
//! sandbox can forge what it prints or how it names a process. Authentication
//! rests on what the sandbox cannot reach:
//!
//! - `codex-forward-guard.sh` runs outside the sandbox. It mints the invocation
//!   id, records it in a ledger the sandbox cannot write, writes it into the
//!   tool call it authorizes, and refuses every other tool command (Bash,
//!   Monitor, PowerShell) quoting a minted id. A stage can read an id but cannot
//!   get it into a tool call.
//! - The kernel fixes a tool call's argv at exec and only the agent's own
//!   children are tool-call launchers. A sandboxed process cannot become a child
//!   of the agent, and cannot rewrite the launcher outside its sandbox.
//!
//! So one direct child of the verified agent whose kernel-visible argv holds
//! the id is the authorized launch. Two such children are refused. Within that
//! launch, trusted installed code runs down to the shallowest runner: the
//! supervisor (`python3 _codex-direct.py`) or the companion (`node
//! codex-companion.mjs`). The interpreter word matters: the wrapper's argv holds
//! the stage-written prompt, which can name either script.
//!
//! Direct lane: only CPU under the supervisor is evidence. Its own polling is not
//! work, and no worker under it means no hold.
//!
//! Companion lane: the worker is detached from the launch, so no process ties a
//! job's pid to it, and the job file and log live under the stage-writable state
//! root. The launch's own `status <job> --wait` is what names the job. That makes
//! a planted job worthless, but a stage can still touch the log of its real job;
//! that stretches the hold only to the end of the authorized launch, inside the
//! same wall window a foreground command's CPU earns.
//!
//! Known limit: where the launcher itself runs the command with no outer process
//! outside the sandbox (macOS `sandbox-exec`, an unsandboxed session), a stage
//! that reads an id from the ledger can `exec` that launcher into a matching
//! argv. Forging one earns only the bounded CPU hold any foreground command
//! already earns. Revisit if the harness exposes a per-tool-call host identity.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::fs::session_files::load_session_exact_unlocked;
use crate::models::session::SessionType;
use crate::orchestrator::terminal::native::{read_pid_entry, NativeBackend};
use crate::process::{process_start_time, verify_process_identity, IdentityStatus};

use super::foreground_progress::{record, within_window, CommandProgress};
use super::running_command::{
    is_command_launcher, process_argv, process_cpu_ticks, process_rows, tree_cpu_ticks, ProcessRow,
};

/// The installed supervisor's script, as it appears in its argv.
const SUPERVISOR_SCRIPT: &str = "_codex-direct.py";

/// The installed companion runtime the wrapper waits through.
const COMPANION_SCRIPT: &str = "codex-companion.mjs";

/// More levels than the launcher, sandbox and wrapper chain above a supervisor.
const MAX_SUPERVISOR_DEPTH: usize = 16;

/// Whether the worker of this authorized invocation has fresh observed CPU
/// progress, within the same idle budget and wall window as any foreground
/// command, measured from the authorization.
pub(crate) fn direct_worker_holds(
    work_dir: &Path,
    session_id: &str,
    invocation_id: &str,
    authorized_at: DateTime<Utc>,
    budget: Duration,
    now: DateTime<Utc>,
) -> bool {
    let budget_secs = budget.as_secs();
    if !within_window(authorized_at, budget_secs, now) {
        return false;
    }
    let Some(sample) =
        stage_agent(work_dir, session_id).and_then(|agent| sample_worker(agent, invocation_id))
    else {
        return false;
    };
    // One checkpoint per invocation: parallel forwards must not reset each other.
    let checkpoint = format!("{session_id}-{invocation_id}");
    record(
        work_dir,
        &checkpoint,
        sample,
        authorized_at,
        budget_secs,
        now,
    )
    .is_ok_and(|observation| observation.holds(authorized_at, budget_secs, now))
}

/// Whether the launch the guard authorized is running the wrapper's wait on
/// exactly this companion job.
pub(crate) fn launch_awaits_job(
    work_dir: &Path,
    session_id: &str,
    invocation_id: &str,
    job_id: &str,
) -> bool {
    stage_agent(work_dir, session_id).is_some_and(|agent| {
        awaits_job(agent, &process_rows(), invocation_id, job_id, process_argv)
    })
}

/// The verified agent process of a stage session that has not ended.
fn stage_agent(work_dir: &Path, session_id: &str) -> Option<u32> {
    let session = load_session_exact_unlocked(work_dir, session_id).ok()??;
    if session.session_type != SessionType::Stage || session.status.is_terminal() {
        return None;
    }
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session)?;
    let agent = read_pid_entry(work_dir, &pid_key)?;
    (verify_process_identity(agent) == IdentityStatus::VerifiedAlive).then_some(agent.pid)
}

fn sample_worker(agent: u32, invocation_id: &str) -> Option<CommandProgress> {
    let rows = process_rows();
    let supervisor = authorized_supervisor(agent, &rows, invocation_id, process_argv)?;
    let cpu_ticks = worker_cpu_ticks(supervisor, &rows, process_cpu_ticks)?;
    Some(CommandProgress {
        pid: supervisor,
        started: Some(process_start_time(supervisor)?),
        cpu_ticks,
    })
}

/// Whether a launcher's argv carries this invocation, which the guard wrote there.
pub(super) fn argv_names_invocation(argv: &str, invocation_id: &str) -> bool {
    argv.contains(&format!("--invocation-id {invocation_id}"))
}

/// The one launcher whose own argv carries this invocation.
fn authorized_launcher(
    agent_pid: u32,
    rows: &[ProcessRow],
    invocation_id: &str,
    argv_of: &impl Fn(u32) -> Option<String>,
) -> Option<u32> {
    let mut launchers = rows
        .iter()
        .filter(|row| row.ppid == agent_pid && !row.zombie && is_command_launcher(&row.name))
        .filter(|row| {
            argv_of(row.pid).is_some_and(|argv| argv_names_invocation(&argv, invocation_id))
        });
    let launcher = launchers.next()?;
    launchers.next().is_none().then_some(launcher.pid)
}

/// The shallowest installed runner under an authorized launch, with its argv.
/// Everything above it is trusted code; a Codex tool runs below it.
fn shallowest_runner(
    launcher: u32,
    rows: &[ProcessRow],
    argv_of: &impl Fn(u32) -> Option<String>,
) -> Option<(u32, String)> {
    let mut level = vec![launcher];
    for _ in 0..MAX_SUPERVISOR_DEPTH {
        let children = live_children(&level, rows);
        let runner = children.iter().find_map(|pid| {
            argv_of(*pid)
                .filter(|argv| is_supervisor(argv) || is_companion(argv))
                .map(|argv| (*pid, argv))
        });
        if runner.is_some() {
            return runner;
        }
        level = children;
    }
    None
}

/// The supervisor of the authorized launch, when it runs the direct lane.
fn authorized_supervisor(
    agent_pid: u32,
    rows: &[ProcessRow],
    invocation_id: &str,
    argv_of: impl Fn(u32) -> Option<String>,
) -> Option<u32> {
    let launcher = authorized_launcher(agent_pid, rows, invocation_id, &argv_of)?;
    let (pid, argv) = shallowest_runner(launcher, rows, &argv_of)?;
    is_supervisor(&argv).then_some(pid)
}

/// Whether the authorized launch's runner is the wrapper's wait on `job_id`.
fn awaits_job(
    agent_pid: u32,
    rows: &[ProcessRow],
    invocation_id: &str,
    job_id: &str,
    argv_of: impl Fn(u32) -> Option<String>,
) -> bool {
    authorized_launcher(agent_pid, rows, invocation_id, &argv_of)
        .and_then(|launcher| shallowest_runner(launcher, rows, &argv_of))
        .is_some_and(|(_, argv)| is_companion(&argv) && names_wait_on(&argv, job_id))
}

/// Linux argv words end in NUL and macOS `ps` joins them with spaces, so a word
/// here is what lies between either. A path holding a space stays recognizable
/// by its last part.
fn argv_words(argv: &str) -> impl Iterator<Item = &str> {
    argv.split(|c: char| c == '\0' || c.is_whitespace())
        .filter(|word| !word.is_empty())
}

fn runs_under(argv: &str, interpreter: &str) -> bool {
    argv_words(argv).next().is_some_and(|word| {
        word.rsplit('/')
            .next()
            .unwrap_or(word)
            .to_ascii_lowercase()
            .starts_with(interpreter)
    })
}

fn is_supervisor(argv: &str) -> bool {
    runs_under(argv, "python") && argv.contains(SUPERVISOR_SCRIPT)
}

fn is_companion(argv: &str) -> bool {
    runs_under(argv, "node") && argv.contains(COMPANION_SCRIPT)
}

/// `<companion> status <job>`, read at the first companion word: a prompt later
/// in the argv can spell out the same words.
fn names_wait_on(argv: &str, job_id: &str) -> bool {
    let mut words = argv_words(argv).skip_while(|word| !word.ends_with(COMPANION_SCRIPT));
    words.next();
    words.next() == Some("status") && words.next() == Some(job_id)
}

/// CPU under the supervisor, excluding its own polling. `None` without a live
/// worker: an open supervisor alone is no evidence.
fn worker_cpu_ticks(
    supervisor: u32,
    rows: &[ProcessRow],
    ticks_of: impl Fn(u32) -> Option<u64>,
) -> Option<u64> {
    let workers = live_children(&[supervisor], rows);
    if workers.is_empty() {
        return None;
    }
    tree_cpu_ticks(&workers, rows, ticks_of)
}

fn live_children(parents: &[u32], rows: &[ProcessRow]) -> Vec<u32> {
    rows.iter()
        .filter(|row| parents.contains(&row.ppid) && !row.zombie)
        .map(|row| row.pid)
        .collect()
}

#[cfg(test)]
#[path = "direct_worker_live_tests.rs"]
pub(crate) mod live_tests;
#[cfg(test)]
#[path = "direct_worker_tests.rs"]
mod tests;
