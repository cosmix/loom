//! The registry of running Codex jobs and the signals that end them.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

use super::FORCE_AFTER;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::pressure::config_guard::ConfigSnapshot;

#[cfg(test)]
mod tests;

/// How often a server stop looks for the groups it waits on to be gone.
const FORCE_POLL: Duration = Duration::from_millis(50);

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
    /// Groups asked to stop and not yet forced. A group stays here after its job left `jobs`:
    /// a descendant in the group can outlive the leader.
    pub(super) stopped_groups: Vec<Pid>,
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

/// Note `group` as asked to stop, once, until a forced stop takes it.
fn note_stopped(registry: &mut Registry, group: Pid) {
    if !registry.stopped_groups.contains(&group) {
        registry.stopped_groups.push(group);
    }
}

/// Take `group` from the groups asked to stop; false when a server stop already took it.
fn take_group(jobs: &Jobs, group: Pid) -> bool {
    let mut registry = lock(jobs);
    let at = registry.stopped_groups.iter().position(|one| *one == group);
    at.map(|at| registry.stopped_groups.swap_remove(at))
        .is_some()
}

/// SIGKILL `group`. A group whose last member has exited answers ESRCH, which means done.
fn kill_group(group: Pid) {
    match killpg(group, Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => {}
        Err(error) => line(&format!(
            "summaries: cannot force the stop of process group {group}: {}",
            safe(&error.to_string())
        )),
    }
}

/// Force `group` after [`FORCE_AFTER`], unless a shutdown took it first. The job may have left
/// the registry by then: its leader's exit says nothing of the other members of the group. The
/// id is still safe to signal, since the kernel reuses no pid while a process group of that id
/// has a member; once the last member is gone the id answers ESRCH, and [`FORCE_AFTER`] bounds
/// how long it is trusted after that.
fn force_later(jobs: &Jobs, group: Pid) {
    let registry = Arc::clone(jobs);
    let forced = thread::Builder::new()
        .name("loom-plan-codex-kill".into())
        .spawn(move || {
            thread::sleep(FORCE_AFTER);
            if take_group(&registry, group) {
                kill_group(group);
            }
        });
    if let Err(error) = forced {
        take_group(jobs, group);
        line(&format!(
            "summaries: cannot schedule the forced stop of the codex session: {}",
            safe(&error.to_string())
        ));
    }
}

/// Ask the job's process group to exit and force it after [`FORCE_AFTER`]; a job that is not
/// running is left alone.
pub(super) fn terminate(jobs: &Jobs, session: &str) {
    let group = {
        let mut registry = lock(jobs);
        let Some(job) = registry.jobs.get_mut(session) else {
            return;
        };
        ask_to_stop(session, job);
        let group = job.pid;
        note_stopped(&mut registry, group);
        group
    };
    force_later(jobs, group);
}

/// Close the registry to new jobs and stop every job in it; [`force_stopped`] finishes the stop.
pub(super) fn terminate_all(jobs: &Jobs) {
    let mut registry = lock(jobs);
    registry.closed = true;
    for (session, job) in registry.jobs.iter_mut() {
        ask_to_stop(session, job);
    }
    let groups: Vec<Pid> = registry.jobs.values().map(|job| job.pid).collect();
    for group in groups {
        note_stopped(&mut registry, group);
    }
}

/// Wait up to `grace` for the groups asked to stop to empty, or until `aborted`, then SIGKILL
/// what remains. For the server's stop, whose exit would take a sleeping [`force_later`] with it.
pub(super) fn force_stopped(jobs: &Jobs, grace: Duration, aborted: &AtomicBool) {
    let deadline = Instant::now() + grace;
    while !aborted.load(Ordering::SeqCst) && Instant::now() < deadline && any_group_left(jobs) {
        thread::sleep(FORCE_POLL);
    }
    let groups = std::mem::take(&mut lock(jobs).stopped_groups);
    for group in groups {
        kill_group(group);
    }
}

fn any_group_left(jobs: &Jobs) -> bool {
    let registry = lock(jobs);
    let gone = |group: &Pid| matches!(killpg(*group, None::<Signal>), Err(Errno::ESRCH));
    !registry.stopped_groups.iter().all(gone)
}
