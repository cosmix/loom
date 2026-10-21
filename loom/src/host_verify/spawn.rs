//! Spawning a job's `verify-host` child for the runner, and reading the
//! outcome an exited child left behind.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result};

use super::runner::RunnerConfig;
use super::{store, JobState, DETAIL_MAX_BYTES};
use crate::fs::work_dir::WorkDir;
use crate::process::OwnedProcess;

/// Variables of the daemon's environment the child never inherits, beside
/// every `LOOM_SESSION*`: it is the daemon's process, never a session's.
const STRIPPED_ENV: [&str; 9] = [
    "LOOM_STAGE_ID",
    "LOOM_SCRATCH_DIR",
    "LOOM_WORKTREE_PATH",
    "LOOM_SESSION_TYPE",
    "LOOM_HOOK_CONTEXT",
    "LOOM_MAIN_AGENT_PID",
    "LOOM_MERGE_SESSION",
    "LOOM_BIN",
    "LOOM_HOOK_PATH",
];

/// Spawn `<program> <args_prefix..> <stage_id> --job <job_id>` in its own
/// process group, from the main project root, with stdout and stderr appended
/// to `child.log`, the daemon's environment minus every session variable, and
/// `LOOM_WORK_DIR` naming the canonical state root. The child resolves
/// nothing from its own working directory. Returns the child and its log.
pub(super) fn spawn_child(
    work_dir: &Path,
    config: &RunnerConfig,
    stage_id: &str,
    job_id: &str,
) -> Result<(OwnedProcess, File)> {
    let root = work_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", work_dir.display()))?;
    let project_root = WorkDir::new(&root)?
        .main_project_root()
        .with_context(|| format!("no project root above {}", root.display()))?;
    let log = store::open_child_log(&root, stage_id, job_id)?;
    let mut command = Command::new(&config.program);
    command
        .args(&config.args_prefix)
        .args([stage_id, "--job", job_id])
        .current_dir(project_root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?)
        .process_group(0);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(is_session_variable) {
            command.env_remove(&key);
        }
    }
    command.env("LOOM_WORK_DIR", &root);
    let child = command
        .spawn()
        .with_context(|| format!("cannot spawn {}", config.program.display()))?;
    Ok((OwnedProcess::new(child), log))
}

/// The state an exited child leaves its job in, from its `result.json`; with
/// none, `Failed` and a detail naming the exit and the end of `child.log`.
pub(super) fn reaped_outcome(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    status: ExitStatus,
    log: &File,
) -> (JobState, Option<String>) {
    let exited = match status.code() {
        Some(code) => format!("verify-host exited {code}"),
        None => format!(
            "verify-host exited on signal {}",
            status.signal().unwrap_or(0)
        ),
    };
    match store::load_result(work_dir, stage_id, job_id) {
        Ok(Some(result)) if result.passed => (JobState::Passed, None),
        Ok(Some(_)) => (JobState::Failed, None),
        Ok(None) => {
            let prefix = format!("{exited} without a result");
            (JobState::Failed, Some(with_log_tail(&prefix, log)))
        }
        Err(error) => {
            let detail = format!("{exited}; its result is unreadable: {error:#}");
            (JobState::Failed, Some(detail))
        }
    }
}

/// `prefix` and the last lines of `log`, within the detail limit.
fn with_log_tail(prefix: &str, log: &File) -> String {
    let budget = DETAIL_MAX_BYTES.saturating_sub(prefix.len() + 2);
    let len = log.metadata().map_or(0, |metadata| metadata.len());
    let start = len.saturating_sub(budget as u64);
    let mut bytes = vec![0; usize::try_from(len - start).unwrap_or(0)];
    if log.read_exact_at(&mut bytes, start).is_err() {
        return prefix.to_string();
    }
    let text = String::from_utf8_lossy(&bytes);
    // A tail that starts mid-file drops its partial first line.
    let text = match text.split_once('\n') {
        Some((_, rest)) if start > 0 => rest,
        _ => &text[..],
    };
    let mut cut = text.len().saturating_sub(budget);
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    match text[cut..].trim() {
        "" => prefix.to_string(),
        tail => format!("{prefix}: {tail}"),
    }
}

/// Whether the child must not inherit `key` from the daemon's environment.
pub(super) fn is_session_variable(key: &str) -> bool {
    key.starts_with("LOOM_SESSION") || STRIPPED_ENV.contains(&key)
}
