//! Daemon-side runner of native worker supervisors.
//!
//! Each admitted invocation gets exactly one `loom subagents supervise`
//! process. The supervisor owns its invocation by creating `supervisor.json`
//! exclusively as its first act (common.md section 4); this runner spawns
//! only when that file is absent and it has not spawned for the invocation in
//! this process already, and it never retries a spawn. Children are held as
//! plain `Child`: a supervisor must outlive the daemon, and nothing here kills
//! one. A supervisor that vanished without settling is settled by
//! `recover::settle_lost_supervisor` once it is proven gone.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;

use super::model::{CancelRequest, NativeEvent, Outcome};
use super::store::{self, EventLog};
use crate::fs::work_dir::WorkDir;
use crate::host_verify::runner::{installed_binary, is_session_variable};

#[path = "runner_recover.rs"]
mod recover;
#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

pub use recover::{recover_after_restart, settle_lost_supervisor};

use recover::{Pass, Step, WarnKey};

/// How long `cancel_all` waits for held supervisors to exit.
const STOP_WAIT: Duration = Duration::from_secs(10);
const STOP_POLL: Duration = Duration::from_millis(100);

/// A supervisor this runner spawned and has not reaped yet.
struct Held {
    dir: PathBuf,
    child: Child,
}

/// Starts, reaps and settles the supervisors of every native worker record.
pub struct NativeRunner {
    work_dir: PathBuf,
    program: Option<PathBuf>,
    held: HashMap<String, Held>,
    /// Invocations this process spawned a supervisor for, held or reaped.
    spawned: HashSet<String>,
    /// Invocations that are terminal with ownership no longer held.
    settled: HashSet<String>,
    /// Warnings the previous pass reported, so a lasting condition logs once.
    warned: HashSet<WarnKey>,
}

impl NativeRunner {
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            work_dir,
            program: None,
            held: HashMap::new(),
            spawned: HashSet::new(),
            settled: HashSet::new(),
            warned: HashSet::new(),
        }
    }

    /// Runs `program` instead of the installed loom binary (tests).
    pub fn with_program(mut self, program: PathBuf) -> Self {
        self.program = Some(program);
        self
    }

    /// One pass: reap exited children, settle lost supervisors, start the
    /// admitted invocations. Never fails; every error is logged and the
    /// affected invocation is looked at again next tick.
    pub fn tick(&mut self) {
        self.reap();
        for step in self.pass(true) {
            if let Step::Spawn { invocation_id, dir } = step {
                self.start(invocation_id, dir);
            }
        }
    }

    /// One recovery pass over the records this runner does not hold.
    fn pass(&mut self, may_spawn: bool) -> Vec<Step> {
        let pass = Pass {
            work_dir: &self.work_dir,
            held: &self.held.keys().cloned().collect(),
            spawned: &self.spawned,
            may_spawn,
        };
        let steps = recover::run_pass(&pass, &mut self.settled, &mut self.warned);
        for step in &steps {
            match step {
                Step::Settled(invocation_id) => {
                    tracing::warn!(%invocation_id, "native worker supervisor lost; record settled")
                }
                Step::Warn(warning) => warning.log(),
                Step::Spawn { .. } => {}
            }
        }
        steps
    }

    /// Daemon stop: asks every supervisor to cancel, waits up to `STOP_WAIT`
    /// for the held ones, settles those that exited and leaves the rest
    /// running (a later daemon settles them through `recover_after_restart`).
    pub fn cancel_all(&mut self) {
        let request = CancelRequest {
            reason: "daemon stopped".to_string(),
            requested_at: Utc::now(),
            by: "daemon".to_string(),
        };
        let held_dirs = self.held.values().map(|held| held.dir.clone());
        let claimed = recover::claimed_running(&self.work_dir);
        for dir in held_dirs.chain(claimed).collect::<HashSet<_>>() {
            if let Err(error) = store::write_cancel(&dir, &request) {
                tracing::warn!(dir = %dir.display(), %error, "native worker cancel failed");
            }
        }
        let deadline = Instant::now() + STOP_WAIT;
        loop {
            self.reap();
            if self.held.is_empty() || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(STOP_POLL);
        }
        self.pass(false);
        for (invocation_id, _) in self.held.drain() {
            tracing::warn!(%invocation_id, "native worker supervisor still running at daemon stop");
        }
    }

    /// Reaps every held child that exited. The next pass settles its record
    /// (a no-op when the supervisor settled it itself): it is then neither
    /// held nor startable, so the pass applies the claim rules to it.
    fn reap(&mut self) {
        let mut exited = Vec::new();
        for (invocation_id, held) in &mut self.held {
            match held.child.try_wait() {
                Ok(Some(status)) => exited.push((invocation_id.clone(), status)),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%invocation_id, %error, "cannot poll native worker supervisor")
                }
            }
        }
        for (invocation_id, status) in exited {
            self.held.remove(&invocation_id);
            tracing::info!(%invocation_id, %status, "native worker supervisor exited");
        }
    }

    /// Spawns the supervisor of one admitted invocation. A failure before the
    /// spawn (log, program, project root) is retried next tick; a failed
    /// spawn ends the record, since no supervisor exists to do it.
    fn start(&mut self, invocation_id: String, dir: PathBuf) {
        let prepared = self.command(&dir);
        let mut command = match prepared {
            Ok(command) => command,
            Err(error) => {
                tracing::warn!(%invocation_id, error = %format!("{error:#}"), "cannot prepare native worker supervisor");
                return;
            }
        };
        self.spawned.insert(invocation_id.clone());
        match command.spawn() {
            Ok(child) => {
                tracing::info!(%invocation_id, pid = child.id(), "native worker supervisor started");
                self.held.insert(invocation_id, Held { dir, child });
            }
            Err(error) => {
                if let Err(append) = fail_start(&dir, &error) {
                    tracing::warn!(%invocation_id, error = %format!("{append:#}"), "cannot record the failed start");
                }
            }
        }
    }

    /// `<program> subagents supervise --dir <dir>` in its own session, from
    /// the main project root, with stdout and stderr appended to
    /// `supervisor.log` and no session variable of the daemon.
    fn command(&self, dir: &Path) -> Result<Command> {
        let root = self
            .work_dir
            .canonicalize()
            .with_context(|| format!("cannot resolve {}", self.work_dir.display()))?;
        let project_root = WorkDir::new(&root)?
            .main_project_root()
            .with_context(|| format!("no project root above {}", root.display()))?;
        let program = match &self.program {
            Some(program) => program.clone(),
            None => installed_binary(std::env::current_exe().context("cannot locate loom")?),
        };
        let log = open_log(dir)?;
        let mut command = Command::new(program);
        command
            .args(["subagents", "supervise", "--dir"])
            .arg(dir)
            .current_dir(project_root)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        for (key, _) in std::env::vars_os() {
            if key.to_str().is_some_and(is_session_variable) {
                command.env_remove(&key);
            }
        }
        command.env("LOOM_WORK_DIR", &root);
        // SAFETY: the closure runs in the forked child before exec and calls
        // only setsid(2), which is async-signal-safe; it allocates nothing.
        // No `process_group(0)`: a group leader cannot setsid (EPERM).
        unsafe {
            command.pre_exec(|| {
                nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(std::io::Error::from)
            });
        }
        Ok(command)
    }
}

/// `supervisor.log`, append-only, never through a symlink.
fn open_log(dir: &Path) -> Result<File> {
    OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(dir.join("supervisor.log"))
        .with_context(|| format!("cannot open {}/supervisor.log", dir.display()))
}

/// Ends a record whose supervisor could not be spawned. The tombstone comes
/// first: when another supervisor (one an earlier daemon started) already
/// claimed the invocation, that one owns the record and nothing is written.
fn fail_start(dir: &Path, error: &std::io::Error) -> Result<()> {
    if !recover::tombstone(dir) {
        tracing::warn!(dir = %dir.display(), %error, "supervisor spawn failed; supervisor.json exists or cannot be created");
        return Ok(());
    }
    let mut log = EventLog::open(dir).context("events.jsonl")?;
    log.append(NativeEvent::Terminal {
        outcome: Outcome::Failed,
        reason: format!("cannot start the supervisor: {error}"),
    })
    .context("terminal event")?;
    log.append(NativeEvent::OwnershipReleased {
        by: "daemon".to_string(),
    })
    .context("release event")?;
    Ok(())
}
