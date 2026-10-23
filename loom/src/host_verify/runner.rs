//! The daemon's host-job runner: starts each queued job as a
//! `loom stage verify-host` child, polls it with `try_wait`, and ends it on
//! cancellation, deadline or output cap (common.md 3.4).
//!
//! The orchestrator tick is single-threaded, so `tick` never waits on a
//! child; retiring one blocks at most `process::owned::CLEANUP_GRACE`. Every
//! record change is a compare-and-set under the stage lock: the record is
//! reloaded and changed only when it is still this job in the expected state,
//! so a cancellation written by another process is never overwritten.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;

use super::signals::{kill_groups, retire};
use super::spawn::{reaped_outcome, spawn_child};
use super::{bounded_detail, store, JobRecord, JobState};
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::process::OwnedProcess;

pub use super::signals::recover_after_restart;

/// Jobs the daemon runs at once.
const DEFAULT_SLOTS: usize = 2;
/// The detail of a job the graceful shutdown cancelled. Unlike a superseding
/// cancel, no successor job exists, so a freeze job carrying it is recorded as refused.
pub(crate) const DAEMON_STOPPED: &str = "daemon stopped";
/// `child.log` past this size ends the job.
const CHILD_LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Seconds a command without its own timeout counts toward a deadline.
const COMMAND_SECS: u64 = crate::verify::DEFAULT_COMMAND_TIMEOUT.as_secs();

/// What the runner spawns: `<program> <args_prefix..> <stage> --job <id>`.
pub struct RunnerConfig {
    pub program: PathBuf,
    pub args_prefix: Vec<String>,
    pub slots: usize,
}

impl RunnerConfig {
    /// The running loom binary's `stage verify-host`, two jobs at once.
    pub fn daemon_default() -> Result<Self> {
        let program = std::env::current_exe().context("cannot locate the loom binary")?;
        Ok(Self {
            program: installed_binary(program),
            args_prefix: vec!["stage".to_string(), "verify-host".to_string()],
            slots: DEFAULT_SLOTS,
        })
    }
}

/// A job whose child this runner holds.
struct RunningJob {
    stage_id: String,
    process: OwnedProcess,
    log: File,
    started: Instant,
    deadline: Duration,
}

/// Owns one child per running job; dropping the runner kills them all.
pub struct HostJobRunner {
    work_dir: PathBuf,
    config: RunnerConfig,
    deadline_override: Option<Duration>,
    running: HashMap<String, RunningJob>,
}

/// The outcome of a compare-and-set on a job's record.
enum Cas {
    Applied(JobRecord),
    /// The record moved on; this is the job's record as it stands, if any.
    Refused(Option<JobRecord>),
}

impl HostJobRunner {
    pub fn new(work_dir: PathBuf, config: RunnerConfig) -> Self {
        Self {
            work_dir,
            config,
            deadline_override: None,
            running: HashMap::new(),
        }
    }

    /// Give every job of this runner `deadline` instead of
    /// [`job_deadline_secs`]; tests use it.
    pub fn with_deadline_override(mut self, deadline: Duration) -> Self {
        self.deadline_override = Some(deadline);
        self
    }

    /// End cancelled, overdue and oversized jobs, reap exited children, then
    /// start queued jobs while a slot is free. Returns exactly the records of
    /// the jobs this call ended or reaped. A failure on one job is logged and
    /// retried on the next tick; it never stops the others.
    pub fn tick(&mut self) -> Result<Vec<JobRecord>> {
        let mut ended = Vec::new();
        let held: Vec<String> = self.running.keys().cloned().collect();
        for job_id in held {
            match self.check_held(&job_id) {
                Ok(record) => ended.extend(record),
                Err(error) => warn_job(&job_id, "could not settle a host job", &error),
            }
        }
        // Records settled above are already final; a failure to list or start
        // queued jobs must not drop them.
        match self.start_queued() {
            Ok(started) => ended.extend(started),
            Err(error) => warn_job("-", "could not start queued host jobs", &error),
        }
        Ok(ended)
    }

    /// The graceful stop: cancel every running record with `daemon stopped`,
    /// so waiters stop waiting, then kill each child and its criteria.
    pub fn shutdown(&mut self) {
        let work_dir = &self.work_dir;
        for (job_id, mut job) in self.running.drain() {
            let stage_id = job.stage_id.as_str();
            let stopped = compare_and_set(work_dir, stage_id, &job_id, JobState::Running, |r| {
                end_record(r, JobState::Cancelled, Some(DAEMON_STOPPED.to_string()));
            });
            if let Err(error) = stopped {
                warn_job(&job_id, "could not mark a host job stopped", &error);
            }
            retire(work_dir, stage_id, &job_id, &mut job.process);
        }
    }

    /// Steps 1 and 2 of the tick for one held job. The job stays held until
    /// its record is settled, so an error is retried on the next tick.
    fn check_held(&mut self, job_id: &str) -> Result<Option<JobRecord>> {
        let job = self.running.get_mut(job_id).context("job not held")?;
        let stage_id = job.stage_id.clone();
        // The deadline and output cap come first: a corrupt record must not
        // keep a runaway child alive.
        if let Some(detail) = overrun(job) {
            retire(&self.work_dir, &stage_id, job_id, &mut job.process);
            return self.settle(&stage_id, job_id, JobState::Failed, Some(detail));
        }
        let current = store::load_current(&self.work_dir, &stage_id)?;
        if !current
            .as_ref()
            .is_some_and(|r| r.id == job_id && r.state == JobState::Running)
        {
            retire(&self.work_dir, &stage_id, job_id, &mut job.process);
            self.running.remove(job_id);
            return match current.filter(|r| r.id == job_id) {
                Some(record) => Ok(Some(record)),
                None => store::load_job_record(&self.work_dir, &stage_id, job_id),
            };
        }
        let Some(status) = job.process.try_wait()? else {
            return Ok(None);
        };
        let reaped = reaped_outcome(&self.work_dir, &stage_id, job_id, status, &job.log);
        if reaped.without_result {
            // A child that died mid-run never stopped its criteria, which
            // lead groups of their own.
            kill_groups(&self.work_dir, &stage_id, job_id);
        }
        self.settle(&stage_id, job_id, reaped.state, reaped.detail)
    }

    /// End a held job's record and release the job.
    fn settle(
        &mut self,
        stage_id: &str,
        job_id: &str,
        state: JobState,
        detail: Option<String>,
    ) -> Result<Option<JobRecord>> {
        let cas = compare_and_set(&self.work_dir, stage_id, job_id, JobState::Running, |r| {
            end_record(r, state, detail);
        })?;
        self.running.remove(job_id);
        Ok(match cas {
            Cas::Applied(record) => Some(record),
            Cas::Refused(record) => record,
        })
    }

    /// Step 3: start queued jobs, oldest first, while a slot is free.
    fn start_queued(&mut self) -> Result<Vec<JobRecord>> {
        let mut ended = Vec::new();
        if self.running.len() >= self.config.slots {
            return Ok(ended);
        }
        for record in queued_records(&self.work_dir)? {
            if self.running.len() >= self.config.slots {
                break;
            }
            match self.start(&record) {
                Ok(failed) => ended.extend(failed),
                Err(error) => warn_job(&record.id, "could not start a host job", &error),
            }
        }
        Ok(ended)
    }

    /// Start one queued job: `Running` and `started_at` before the spawn,
    /// `pid` after it. Returns the record when the job failed to start.
    fn start(&mut self, record: &JobRecord) -> Result<Option<JobRecord>> {
        let (stage_id, job_id) = (record.stage_id.as_str(), record.id.as_str());
        let deadline = match crate::verify::transitions::load_stage(stage_id, &self.work_dir) {
            Ok(stage) => self
                .deadline_override
                .unwrap_or_else(|| Duration::from_secs(job_deadline_secs(&stage))),
            Err(error) => {
                let detail = format!("cannot load stage '{stage_id}': {error:#}");
                return self.fail_to_start(stage_id, job_id, JobState::Queued, detail);
            }
        };
        let started = compare_and_set(&self.work_dir, stage_id, job_id, JobState::Queued, |r| {
            r.state = JobState::Running;
            r.started_at = Some(Utc::now());
        })?;
        if !matches!(started, Cas::Applied(_)) {
            return Ok(None);
        }
        let (process, log) = match spawn_child(&self.work_dir, &self.config, stage_id, job_id) {
            Ok(spawned) => spawned,
            Err(error) => {
                let detail = format!("cannot start verify-host: {error:#}");
                return self.fail_to_start(stage_id, job_id, JobState::Running, detail);
            }
        };
        let mut job = RunningJob {
            stage_id: stage_id.to_string(),
            process,
            log,
            started: Instant::now(),
            deadline,
        };
        let pid = job.process.child().id();
        // Held before the pid is written: a failed write must not orphan it.
        self.running.insert(job_id.to_string(), job);
        let recorded = compare_and_set(&self.work_dir, stage_id, job_id, JobState::Running, |r| {
            r.pid = Some(pid);
        });
        if let Err(error) = recorded {
            warn_job(job_id, "could not record a host job's pid", &error);
        }
        Ok(None)
    }

    fn fail_to_start(
        &self,
        stage_id: &str,
        job_id: &str,
        expected: JobState,
        detail: String,
    ) -> Result<Option<JobRecord>> {
        let cas = compare_and_set(&self.work_dir, stage_id, job_id, expected, |r| {
            end_record(r, JobState::Failed, Some(detail));
        })?;
        Ok(match cas {
            Cas::Applied(record) => Some(record),
            Cas::Refused(_) => None,
        })
    }
}

impl Drop for HostJobRunner {
    /// `OwnedProcess` kills a child's own group on drop; its criteria lead
    /// groups of their own, which only the job's `groups` file names.
    fn drop(&mut self) {
        for (job_id, job) in &mut self.running {
            retire(&self.work_dir, &job.stage_id, job_id, &mut job.process);
        }
    }
}

/// Fail every queued job with `detail`: no runner can start it, and its
/// waiter must not report it pending forever. Returns the records it failed.
pub fn fail_queued(work_dir: &Path, detail: &str) -> Result<Vec<JobRecord>> {
    let mut failed = Vec::new();
    for record in queued_records(work_dir)? {
        let (stage_id, job_id) = (record.stage_id.as_str(), record.id.as_str());
        match compare_and_set(work_dir, stage_id, job_id, JobState::Queued, |r| {
            end_record(r, JobState::Failed, Some(detail.to_string()));
        }) {
            Ok(Cas::Applied(record)) => failed.push(record),
            Ok(Cas::Refused(_)) => {}
            Err(error) => warn_job(job_id, "could not fail a queued host job", &error),
        }
    }
    Ok(failed)
}

/// Every stage's current record that is queued, oldest first.
fn queued_records(work_dir: &Path) -> Result<Vec<JobRecord>> {
    let mut queued = Vec::new();
    for stage_id in store::stage_ids(work_dir)? {
        match store::load_current(work_dir, &stage_id) {
            Ok(Some(record)) if record.state == JobState::Queued => queued.push(record),
            Ok(_) => {}
            Err(error) => warn_job(&stage_id, "could not read a stage's host job", &error),
        }
    }
    queued.sort_by_key(|record| record.queued_at);
    Ok(queued)
}

/// `current_exe` of a binary replaced on disk under the running process (a
/// reinstall) reads `<path> (deleted)` on Linux; the binary now at `<path>`
/// is the one to spawn.
pub(super) fn installed_binary(exe: PathBuf) -> PathBuf {
    exe.to_str()
        .and_then(|path| path.strip_suffix(" (deleted)"))
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .unwrap_or(exe)
}

/// The longest a job of `stage` may run: every acceptance criterion at its
/// own timeout (else 300 s), 300 s for each wiring test, the dead-code check,
/// the regression test, each frozen contract, one impact-test selection and
/// each `after_stage` command, plus the completion margin. A check runs a
/// share of that, so one deadline per stage serves every kind of job.
pub fn job_deadline_secs(stage: &Stage) -> u64 {
    let acceptance = stage.acceptance.iter().fold(0u64, |sum, criterion| {
        let secs = match criterion {
            AcceptanceCriterion::Extended(check) => check.timeout_secs.unwrap_or(COMMAND_SECS),
            AcceptanceCriterion::Simple(_) => COMMAND_SECS,
        };
        sum.saturating_add(secs)
    });
    let commands = stage.wiring_tests.len()
        + usize::from(stage.dead_code_check.is_some())
        + usize::from(stage.regression_test.is_some())
        + stage.contracts.len()
        + 1
        + stage.after_stage.len();
    u64::try_from(commands)
        .unwrap_or(u64::MAX)
        .saturating_mul(COMMAND_SECS)
        .saturating_add(acceptance)
        .saturating_add(crate::orchestrator::monitor::stall_line::COMPLETION_MARGIN_SECS)
}

/// Under the stage lock, apply `change` to the job's record when it is still
/// the stage's current job in state `expected`.
fn compare_and_set(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    expected: JobState,
    change: impl FnOnce(&mut JobRecord),
) -> Result<Cas> {
    let _lock = store::lock_stage(work_dir, stage_id)?;
    match store::load_current(work_dir, stage_id)? {
        Some(mut record) if record.id == job_id && record.state == expected => {
            change(&mut record);
            store::save_current(work_dir, &record)?;
            Ok(Cas::Applied(record))
        }
        Some(record) if record.id == job_id => Ok(Cas::Refused(Some(record))),
        _ => Ok(Cas::Refused(store::load_job_record(
            work_dir, stage_id, job_id,
        )?)),
    }
}

fn end_record(record: &mut JobRecord, state: JobState, detail: Option<String>) {
    record.state = state;
    record.detail = detail.as_deref().map(bounded_detail);
    record.finished_at = Some(Utc::now());
}

/// Why a running job must end now, if it must.
fn overrun(job: &RunningJob) -> Option<String> {
    if job.started.elapsed() > job.deadline {
        return Some(format!("deadline of {} s passed", job.deadline.as_secs()));
    }
    let logged = job.log.metadata().map_or(0, |metadata| metadata.len());
    (logged > CHILD_LOG_MAX_BYTES).then(|| "child output exceeded 4 MiB".to_string())
}

fn warn_job(id: &str, message: &str, error: &anyhow::Error) {
    tracing::warn!(id = %id, error = %format!("{error:#}"), "{message}");
}
