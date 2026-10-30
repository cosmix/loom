//! The registry of running Codex jobs and the signals that end them.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::pressure::config_guard::ConfigSnapshot;

/// A running job: its process group and the configuration it must leave unchanged.
pub(super) struct Job {
    pub(super) pid: Pid,
    pub(super) before: ConfigSnapshot,
    pub(super) codex_before: BTreeMap<PathBuf, String>,
    /// Set under the jobs lock when the job is stopped: the end that follows was asked for.
    pub(super) stopping: bool,
}

/// The running jobs and whether the spawner still accepts new ones. Both live under one lock,
/// so a shutdown cannot slip between a start's check and its registration.
#[derive(Default)]
pub(super) struct Registry {
    pub(super) closed: bool,
    pub(super) jobs: HashMap<String, Job>,
}

pub(super) type Jobs = Arc<Mutex<Registry>>;

/// Every critical section leaves its data consistent, so a poisoned lock is still usable.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Add `job` under `session`; false (nothing added) once the registry is closed.
pub(super) fn register(jobs: &Jobs, session: &str, job: Job) -> bool {
    let mut registry = lock(jobs);
    if registry.closed {
        return false;
    }
    registry.jobs.insert(session.to_owned(), job);
    true
}

/// End a process group that was spawned but never registered, and reap it.
pub(super) fn discard(pid: Pid, child: &mut Child) {
    let _ = killpg(pid, Signal::SIGKILL);
    let _ = child.wait();
}

fn ask_to_stop(session: &str, job: &mut Job) {
    job.stopping = true;
    if let Err(error) = killpg(job.pid, Signal::SIGTERM) {
        line(&format!(
            "summaries: cannot stop session {}: {}",
            safe(session),
            safe(&error.to_string())
        ));
    }
}

/// Force the groups of `sessions` that are still registered after 5 s.
fn force_later(jobs: &Jobs, sessions: Vec<String>) {
    let registry = Arc::clone(jobs);
    let forced = thread::Builder::new()
        .name("loom-plan-codex-kill".into())
        .spawn(move || {
            thread::sleep(Duration::from_secs(5));
            // Hold the registry lock through signalling so removal cannot race it.
            let registry = lock(&registry);
            for job in sessions.iter().filter_map(|id| registry.jobs.get(id)) {
                let _ = killpg(job.pid, Signal::SIGKILL);
            }
        });
    if let Err(error) = forced {
        line(&format!(
            "summaries: cannot schedule the forced stop of the codex sessions: {}",
            safe(&error.to_string())
        ));
    }
}

/// Ask the job's process group to exit and force it after 5 s; a job that is not running is
/// left alone.
pub(super) fn terminate(jobs: &Jobs, session: &str) {
    {
        let mut registry = lock(jobs);
        let Some(job) = registry.jobs.get_mut(session) else {
            return;
        };
        ask_to_stop(session, job);
    }
    force_later(jobs, vec![session.to_owned()]);
}

/// Close the registry to new jobs and stop every job in it.
pub(super) fn terminate_all(jobs: &Jobs) {
    let sessions: Vec<String> = {
        let mut registry = lock(jobs);
        registry.closed = true;
        for (session, job) in registry.jobs.iter_mut() {
            ask_to_stop(session, job);
        }
        registry.jobs.keys().cloned().collect()
    };
    if !sessions.is_empty() {
        force_later(jobs, sessions);
    }
}
