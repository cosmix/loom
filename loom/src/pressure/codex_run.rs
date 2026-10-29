//! Background Codex attempts with in-family model fallback.
//!
//! Account-level model unavailability advances to the next candidate. The
//! runner returns fallback notes and never prints; callers own presentation.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::codex::codex_model_candidates;

/// Phrases Codex and the API print when the account cannot use a model,
/// matched case-insensitively.
const UNAVAILABLE_PHRASES: [&str; 4] = [
    "model is not supported",
    "model not found",
    "model_not_found",
    "does not exist",
];

/// The first error line naming `model` together with an unavailability
/// phrase. Tool output and model prose can quote those phrases, so only
/// `ERROR` lines count. Requiring both on one line also excludes Codex's
/// harmless metadata warning and unrelated errors.
fn unavailable_line<'a>(log: &'a str, model: &str) -> Option<&'a str> {
    log.lines().find(|line| {
        let lower = line.to_lowercase();
        line.trim_start().starts_with("ERROR")
            && line.contains(model)
            && UNAVAILABLE_PHRASES.iter().any(|p| lower.contains(p))
    })
}

/// What the Codex runner reports once its last attempt has exited.
pub struct CodexOutcome {
    /// Exit status of the last attempt.
    pub status: ExitStatus,
    /// Model the last attempt ran with.
    pub model_used: String,
    /// One note per fallback, carrying the error line before the log is replaced.
    pub notes: Vec<String>,
    /// Every model attempted, in order.
    pub tried: Vec<String>,
    /// The last attempt also failed as unavailable and no candidate remained.
    pub exhausted: bool,
}

/// A background Codex run working through its candidate models.
pub struct CodexRun {
    handle: JoinHandle<Result<CodexOutcome>>,
    pid: Arc<AtomicU32>,
}

impl CodexRun {
    /// The running attempt's PID, or `0` between attempts and after exit.
    pub fn current_pid(&self) -> u32 {
        self.pid.load(Ordering::Acquire)
    }

    /// Whether the runner has finished all its attempts.
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Wait for the outcome, turning a runner-thread panic into an error.
    pub fn join(self) -> Result<CodexOutcome> {
        self.handle
            .join()
            .map_err(|_| anyhow!("codex runner thread panicked"))?
    }
}

/// Owned attempt inputs that outlive the caller's borrows on the runner thread.
pub struct CodexJob {
    codex_path: PathBuf,
    repo_root: PathBuf,
    skill: String,
    log_path: PathBuf,
    effort: String,
    pid: Arc<AtomicU32>,
}

impl CodexJob {
    /// Spawn one attempt with `model`; the log file is re-created.
    fn spawn(&self, model: &str) -> Result<Child> {
        let child = spawn_codex_background(
            &self.codex_path,
            &self.repo_root,
            &self.skill,
            &self.log_path,
            model,
            &self.effort,
        )?;
        self.pid.store(child.id(), Ordering::Release);
        Ok(child)
    }

    /// An unreadable log counts as no unavailable-model error, so an unrelated
    /// failure is reported instead of being retried blindly.
    fn unavailable_line(&self, model: &str) -> Option<String> {
        let bytes = std::fs::read(&self.log_path).ok()?;
        let log = String::from_utf8_lossy(&bytes);
        unavailable_line(&log, model).map(|line| line.trim().to_string())
    }
}

/// Start the first candidate on the caller's thread so spawn errors arrive
/// before the foreground session starts. The runner thread tries subsequent
/// candidates with the same effort, returning notes without printing.
pub fn spawn_codex_with_fallback(
    codex_path: &Path,
    repo_root: &Path,
    skill: &str,
    log_path: &Path,
    candidates: Vec<String>,
    effort: &str,
) -> Result<CodexRun> {
    let pid = Arc::new(AtomicU32::new(0));
    let job = CodexJob {
        codex_path: codex_path.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        skill: skill.to_string(),
        log_path: log_path.to_path_buf(),
        effort: effort.to_string(),
        pid: Arc::clone(&pid),
    };
    let first = candidates.first().context("no codex model to run")?;
    let child = job.spawn(first)?;
    let handle = thread::spawn(move || run_attempts(&job, child, &candidates));
    Ok(CodexRun { handle, pid })
}

/// Wait on each attempt, retrying only when the account lacks its model.
fn run_attempts(job: &CodexJob, mut child: Child, candidates: &[String]) -> Result<CodexOutcome> {
    let mut notes = Vec::new();
    let mut index = 0;
    loop {
        let model = &candidates[index];
        let status = child.wait();
        job.pid.store(0, Ordering::Release);
        let status = status.context("failed to wait for codex")?;
        let unavailable = if status.success() {
            None
        } else {
            job.unavailable_line(model)
        };
        match (unavailable, candidates.get(index + 1)) {
            (Some(line), Some(next)) => {
                notes.push(fallback_note(model, next, &line));
                child = job
                    .spawn(next)
                    .with_context(|| format!("failed to spawn codex with fallback model {next}"))?;
                index += 1;
            }
            (unavailable, _) => {
                return Ok(CodexOutcome {
                    status,
                    model_used: model.clone(),
                    notes,
                    tried: candidates[..=index].to_vec(),
                    exhausted: unavailable.is_some(),
                });
            }
        }
    }
}

/// Record the fallback with its triggering Codex error line indented below it.
fn fallback_note(from: &str, to: &str, error_line: &str) -> String {
    format!(
        "codex model {from} is unavailable to this account — fell back to {to}\n    {error_line}"
    )
}

/// The model plus its fallback chain, e.g. `gpt-6.1-sol (fallback: gpt-6-sol)`.
pub fn codex_model_label(model: &str) -> String {
    match codex_model_candidates(model).split_first() {
        Some((first, rest)) if !rest.is_empty() => {
            format!("{first} (fallback: {})", rest.join(", "))
        }
        _ => model.to_string(),
    }
}

/// Arguments after the binary for a Codex spawn. `skill` is the full
/// positional invocation, e.g. `$pressure doc/plans/PLAN-foo.md`; `model`
/// and `effort` are the resolved [`super::models::PressureModels`] slots.
/// Codex takes effort through a `-c` override rather than a dedicated flag.
pub fn codex_args(repo_root: &Path, skill: &str, model: &str, effort: &str) -> Vec<String> {
    vec![
        "exec".to_string(),
        "--sandbox".to_string(),
        "workspace-write".to_string(),
        "-m".to_string(),
        model.to_string(),
        "-c".to_string(),
        format!("model_reasoning_effort={effort}"),
        "-C".to_string(),
        repo_root.display().to_string(),
        skill.to_string(),
    ]
}

/// Spawn `codex exec` with output captured to `log_path`. Each call
/// re-creates the log so fallback attempts never mix their output.
pub fn spawn_codex_background(
    codex_path: &Path,
    repo_root: &Path,
    skill: &str,
    log_path: &Path,
    model: &str,
    effort: &str,
) -> Result<Child> {
    let log = std::fs::File::create(log_path)
        .with_context(|| format!("failed to create codex log {}", log_path.display()))?;
    let log_err = log
        .try_clone()
        .context("failed to clone codex log handle")?;
    let mut cmd = Command::new(codex_path);
    cmd.args(codex_args(repo_root, skill, model, effort));
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.spawn().context("failed to spawn codex")
}

/// Whether `$CODEX_HOME/skills/pressure/SKILL.md` is installed as a file.
/// An unset `CODEX_HOME` defaults to `~/.codex`.
pub fn codex_skill_installed() -> bool {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")));
    home.is_some_and(|home| home.join("skills/pressure/SKILL.md").is_file())
}

#[cfg(test)]
mod tests;
