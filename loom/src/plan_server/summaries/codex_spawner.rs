//! Summary jobs running directly under Codex, outside the session bridge.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

mod guard;
mod registry;

use self::guard::{codex_snapshot, guarded_outcome};
use self::registry::{
    discard, force_stopped, lock, register, terminate, terminate_all, Job, Jobs, Registry,
};
use super::rebuilds::Rebuilds;
use super::scheduler::SessionSpawner;
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::pressure::config_guard::{self, ConfigSnapshot};
use crate::plan_session::bridge::EndOutcome;
use crate::plan_session::sandbox::ensure_runtime_grants;
use crate::planning::runtime::{RuntimePaths, DRAFTS_DIR};
use crate::planning::summary::claims::clear_session_claims;
use crate::planning::summary::config::SummaryModels;
use crate::pressure::codex_run::{codex_exec_args, spawn_codex_logged};

/// The claims directory's name under the runtime directory, as `RuntimePaths` joins it.
const CLAIMS_DIR: &str = "claims";

/// The `codex exec` command line for a summary job: the project is the working directory and
/// only `drafts/` and `claims/` under `runtime` are writable besides it, the two directories the
/// Claude summary sandbox grants. The rest of `runtime` holds other sessions' permission
/// answers, settings and launch scripts and stays read-only; the sandbox is never widened.
pub fn codex_summary_args(
    model: &str,
    effort: &str,
    root: &Path,
    runtime: &Path,
    prompt: &str,
) -> Vec<String> {
    let (drafts, claims) = (runtime.join(DRAFTS_DIR), runtime.join(CLAIMS_DIR));
    codex_exec_args(root, &[&drafts, &claims], prompt, model, effort)
}

/// The job prompt with the assigned ids and the runtime directory filled in.
pub(super) fn render_prompt(ids: &[String], runtime: &Path) -> String {
    let ids = ids
        .iter()
        .map(|id| format!("- {id}"))
        .collect::<Vec<_>>()
        .join("\n");
    include_str!("../../planning/guidance/text/summary_codex.md")
        .replace("{{RUNTIME}}", &runtime.display().to_string())
        .replace("{{IDS}}", &ids)
}

/// A clean exit closes the session; any other exit fails it with the log's last line.
pub(super) fn exit_outcome(success: bool, code: Option<i32>, log_tail: &str) -> EndOutcome {
    if success {
        return EndOutcome::Closed;
    }
    let reason = log_tail
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(200).collect())
        .unwrap_or_else(|| match code {
            Some(code) => format!("codex exited with status {code}"),
            None => "codex was killed by a signal".to_string(),
        });
    EndOutcome::Failed(reason)
}

/// Everything a start prepares before the process spawns.
struct Launch {
    session: String,
    log: PathBuf,
    args: Vec<String>,
    before: ConfigSnapshot,
    codex_before: BTreeMap<PathBuf, String>,
}

/// Session ids of Codex jobs start with this; a Claude session id is a UUID, whose first dash
/// is at index 8. Only hex digits and dashes, as `LOOM_PLAN_SESSION` must be for
/// `loom plan summary` to accept it (`planning/runtime.rs`, `check_state`).
const SESSION_PREFIX: &str = "c0de-";

/// Logs kept under `<runtime>/codex-logs`; older ones are removed when a job starts.
const KEPT_LOGS: usize = 20;

/// How long a process group asked to stop has before it is forced.
pub(super) const FORCE_AFTER: Duration = Duration::from_secs(5);

/// True for a session id this spawner issues, whether or not its job still runs.
pub(super) fn is_codex_session(session: &str) -> bool {
    session.starts_with(SESSION_PREFIX)
}

/// The directory of the job logs under the runtime directory.
fn logs_dir(runtime: &RuntimePaths) -> PathBuf {
    runtime.dir.join("codex-logs")
}

/// The log of `session` in the logs directory.
fn log_of(logs: &Path, session: &str) -> PathBuf {
    logs.join(format!("{session}.log"))
}

/// A job stopped on request ends as a teardown whatever its exit status, so the scheduler
/// requeues its ids instead of failing them. Any other end keeps its exit outcome; a clean exit
/// with ids unsealed is bounded by the scheduler (`Scheduler::job_ended`).
pub(super) fn final_outcome(stopping: bool, outcome: EndOutcome) -> EndOutcome {
    if stopping {
        EndOutcome::TornDown
    } else {
        outcome
    }
}

/// Runs each summary batch as one background `codex exec` job and reports its end to the
/// rebuild scheduler, since the session bridge does not know the job.
pub struct CodexSummarySpawner {
    ctx: Weak<PlanContext>,
    counter: AtomicU64,
    jobs: Jobs,
}

impl CodexSummarySpawner {
    pub fn new(ctx: &Arc<PlanContext>) -> Self {
        Self {
            ctx: Arc::downgrade(ctx),
            counter: AtomicU64::new(0),
            jobs: Arc::new(Mutex::new(Registry::default())),
        }
    }

    fn watch(
        &self,
        ctx: &Arc<PlanContext>,
        session: &str,
        child: Child,
        log: PathBuf,
    ) -> Result<()> {
        // Keep ownership available for cleanup if creating the watcher thread fails.
        let child = Arc::new(Mutex::new(child));
        let waiting = Arc::clone(&child);
        let jobs = Arc::clone(&self.jobs);
        let weak = Arc::downgrade(ctx);
        let id = session.to_string();
        let root = ctx.root().to_path_buf();
        let watcher = thread::Builder::new()
            .name("loom-plan-codex-summary".into())
            .spawn(move || {
                let status = lock(&waiting).wait();
                let outcome = settle_outcome(&jobs, &root, &id, status, &log);
                if let Some(ctx) = weak.upgrade() {
                    if let Some(rebuilds) = ctx.extensions().get::<Rebuilds>() {
                        rebuilds.codex_session_ended(&id, &outcome);
                    }
                }
            });
        if let Err(error) = watcher {
            if let Some(job) = lock(&self.jobs).jobs.remove(session) {
                let _ = killpg(job.pid, Signal::SIGKILL);
            }
            let _ = lock(&child).wait();
            return Err(error).context("cannot start codex summary watcher");
        }
        Ok(())
    }
}

impl CodexSummarySpawner {
    /// A new job's session id.
    fn next_session(&self) -> Result<String> {
        let millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{SESSION_PREFIX}{millis}-{counter}"))
    }

    /// The session id, log path, command line and configuration snapshots of a new job, with
    /// its sandbox grants in place: `--add-dir` needs the directories, and the job cannot
    /// create the lock file it locks.
    fn prepare(&self, ctx: &Arc<PlanContext>, ids: &[String]) -> Result<Launch> {
        let models = SummaryModels::load(ctx.root())?;
        let session = self.next_session()?;
        ensure_runtime_grants(ctx.runtime()).context("cannot create the codex summary grants")?;
        let logs = logs_dir(ctx.runtime());
        std::fs::create_dir_all(&logs).context("cannot create codex summary log directory")?;
        let before =
            config_guard::snapshot(ctx.root()).context("cannot snapshot session configuration")?;
        let codex_before =
            codex_snapshot(ctx.root()).context("cannot snapshot .codex configuration")?;
        let prompt = render_prompt(ids, &ctx.runtime().dir);
        let args = codex_summary_args(
            &models.codex_model,
            &models.codex_effort,
            ctx.root(),
            &ctx.runtime().dir,
            &prompt,
        );
        let log = log_of(&logs, &session);
        Ok(Launch {
            session,
            log,
            args,
            before,
            codex_before,
        })
    }
}

impl SessionSpawner for CodexSummarySpawner {
    fn start_summary(&self, ids: &[String]) -> Result<String> {
        let ctx = self.ctx.upgrade().context("the planning server is gone")?;
        if ctx.is_stopping() {
            bail!("loom plan is stopping; no new summary session starts");
        }
        let codex =
            crate::codex::find_codex_path().context("codex summary binary is unavailable")?;
        let launch = self.prepare(&ctx, ids)?;
        let Launch {
            session,
            log,
            args,
            before,
            codex_before,
        } = launch;
        let mut child = spawn_codex_logged(&codex, args, ctx.root(), &log, |command| {
            // A planning job never carries a stage's identity, as the session launch script does.
            command
                .env_remove("LOOM_STAGE_ID")
                .env_remove("LOOM_SESSION_ID")
                .env_remove("LOOM_WORK_DIR")
                .env("LOOM_PLAN_STATE", &ctx.runtime().dir)
                .env("LOOM_PLAN_SESSION", &session)
                .process_group(0);
        })
        .context("cannot spawn codex summary session")?;
        let pid = Pid::from_raw(child.id() as i32);
        let job = Job {
            pid,
            before,
            codex_before,
            stopping: false,
        };
        if !register(&self.jobs, &session, job) {
            discard(pid, &mut child);
            bail!("loom plan is stopping; no new summary session starts");
        }
        prune_logs(&logs_dir(ctx.runtime()));
        self.watch(&ctx, &session, child, log.clone())?;
        line(&format!(
            "summaries: codex session {} logs to {}",
            safe(&session),
            safe(&log.display().to_string())
        ));
        Ok(session)
    }

    fn stop(&self, session: &str, release_claims: bool) {
        terminate(&self.jobs, session);
        if release_claims {
            if let Some(ctx) = self.ctx.upgrade() {
                if let Err(error) = clear_session_claims(ctx.runtime(), session) {
                    line(&format!(
                        "summaries: cannot release the claims of {}: {}",
                        safe(session),
                        safe(&format!("{error:#}"))
                    ));
                }
            }
        }
    }

    fn runs_outside_bridge(&self, session: &str) -> bool {
        lock(&self.jobs).jobs.contains_key(session)
    }

    fn stop_all(&self) {
        terminate_all(&self.jobs);
    }

    fn force_stopped(&self, grace: Duration, aborted: &AtomicBool) {
        force_stopped(&self.jobs, grace, aborted);
    }

    fn log_path(&self, session: &str) -> Option<PathBuf> {
        let ctx = self.ctx.upgrade().filter(|_| is_codex_session(session))?;
        Some(log_of(&logs_dir(ctx.runtime()), session))
    }
}

/// The job's end as the scheduler hears it: the exit status (or a stop on request), checked
/// against the configuration guard; the job leaves the registry once its outcome is known.
fn settle_outcome(
    jobs: &Jobs,
    root: &Path,
    id: &str,
    status: std::io::Result<ExitStatus>,
    log: &Path,
) -> EndOutcome {
    let outcome = match status {
        Ok(status) => exit_outcome(status.success(), status.code(), &log_tail(log)),
        Err(error) => EndOutcome::Failed(format!("cannot wait for codex: {error}")),
    };
    // The job stays registered while the guard reads the configuration, so the liveness tick
    // still sees it run; `stopping` is read from the job at its removal, so a stop that arrives
    // while the guard runs still counts.
    let snapshot = lock(jobs)
        .jobs
        .get(id)
        .map(|job| (job.before.clone(), job.codex_before.clone()));
    let guarded =
        snapshot.and_then(|(before, codex_before)| guarded_outcome(root, &before, &codex_before));
    let stopping = lock(jobs).jobs.remove(id).is_some_and(|job| job.stopping);
    match guarded {
        // A changed or unreadable configuration outranks a requested stop.
        Some(failure) => failure,
        None => final_outcome(stopping, outcome),
    }
}

/// Keep the newest [`KEPT_LOGS`] files of `dir`. A failure is logged and never fails a start.
fn prune_logs(dir: &Path) {
    let prune = || -> std::io::Result<()> {
        let mut logs = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                logs.push((entry.metadata()?.modified()?, entry.path()));
            }
        }
        logs.sort_by(|a, b| b.cmp(a));
        for (_, path) in logs.into_iter().skip(KEPT_LOGS) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    };
    if let Err(error) = prune() {
        line(&format!(
            "summaries: cannot prune the codex logs: {}",
            safe(&error.to_string())
        ));
    }
}

fn log_tail(path: &Path) -> String {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let offset = file.metadata()?.len().saturating_sub(2048);
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(2048).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    String::from_utf8_lossy(&read().unwrap_or_default()).into_owned()
}

#[cfg(test)]
mod launch_tests;
#[cfg(test)]
#[path = "codex_spawner_tests.rs"]
mod tests;
