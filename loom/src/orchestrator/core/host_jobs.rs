//! The host-job runner on the orchestrator tick (common.md 3.4): a pass of
//! the runner after the session inboxes drain, recovery once at startup, and
//! a graceful stop when the run ends. Every job the runner ends is applied
//! here (`host_verify::apply::apply_finished`), and so is every job a restart
//! interrupted or left unapplied. While no runner can be built, each tick
//! tries again and fails every queued job with the reason.
//!
//! A failed application stays in its job directory with a durable attempt
//! count. Each tick scans unmarked terminal jobs, including ones replaced as
//! `current.json`, and the retry owner writes the final outcome at its cap.

use std::path::Path;

use anyhow::Result;

use super::Orchestrator;
use crate::host_verify::apply_retry::{apply_pending, apply_record};
use crate::host_verify::runner::{fail_queued, HostJobRunner, RunnerConfig};
use crate::host_verify::submit::cancel;
use crate::host_verify::JobRecord;

/// The runner the daemon starts with: the running loom binary's
/// `stage verify-host`. `None`, logged, when that binary cannot be located:
/// host jobs then wait for [`retry_runner`] instead of stopping the run.
pub(super) fn daemon_runner(work_dir: &Path) -> Option<HostJobRunner> {
    match RunnerConfig::daemon_default() {
        Ok(config) => Some(HostJobRunner::new(work_dir.to_path_buf(), config)),
        Err(error) => {
            tracing::error!(
                error = %format!("{error:#}"),
                "host jobs are disabled for this run: no verify-host runner",
            );
            None
        }
    }
}

impl Orchestrator {
    /// One pass of the host-job runner: retry the applications that failed,
    /// then end, reap and start jobs, applying every job that ended. A runner
    /// or apply error is logged and never propagated: a broken job must not
    /// stop the orchestrator.
    pub fn tick_host_jobs(&mut self) {
        let work_dir = self.config.work_dir.clone();
        apply_pending(&work_dir);
        if self.host_jobs.is_none() {
            self.host_jobs = retry_runner(&work_dir);
        }
        let Some(runner) = self.host_jobs.as_mut() else {
            return;
        };
        match runner.tick() {
            Ok(ended) => {
                log_records("host job ended", &ended);
                for record in &ended {
                    apply_record(&work_dir, record);
                }
            }
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), "host job tick failed");
            }
        }
    }

    /// End the job in `stage_id`'s worktree before a landing's gates run there.
    /// Its criteria share that worktree, and this tick does not reach the
    /// runner until the gates finish, so a child the runner holds is killed
    /// here. A child that outlives this is an `Err`: the gates must not run.
    pub(super) fn stop_running_check(&mut self, stage_id: &str) -> Result<(), String> {
        let reason = "the resolved merge is landing; its gates run instead";
        cancel(&self.config.work_dir, stage_id, reason).map_err(|error| {
            format!("the stage's running check could not be cancelled: {error:#}")
        })?;
        if !self.holds_stage_job(stage_id) {
            return Ok(());
        }
        self.settle_host_jobs();
        if self.holds_stage_job(stage_id) {
            return Err(format!(
                "the running check of stage '{stage_id}' could not be stopped"
            ));
        }
        Ok(())
    }

    fn holds_stage_job(&self, stage_id: &str) -> bool {
        self.host_jobs
            .as_ref()
            .is_some_and(|runner| runner.holds_stage(stage_id))
    }

    /// The pass of [`Self::tick_host_jobs`] that ends and reaps jobs, starting
    /// none, applying every job that ended.
    fn settle_host_jobs(&mut self) {
        let Some(runner) = self.host_jobs.as_mut() else {
            return;
        };
        let ended = runner.settle_held();
        log_records("host job ended", &ended);
        for record in &ended {
            apply_record(&self.config.work_dir, record);
        }
    }

    /// Replace the runner `Orchestrator::new` built; the replaced one is
    /// stopped first.
    pub fn set_host_job_runner(&mut self, runner: HostJobRunner) {
        if let Some(mut replaced) = self.host_jobs.replace(runner) {
            replaced.shutdown();
        }
    }

    /// Log what `recover_after_restart` changed at startup, then apply every
    /// job it interrupted and every terminal job left unapplied, so a freeze
    /// interrupted by the restart records its refusal instead of stranding a
    /// writer that has already ended its turn.
    pub(super) fn report_host_job_recovery(&self, recovered: Result<Vec<JobRecord>>) {
        let work_dir = &self.config.work_dir;
        match recovered {
            Ok(records) => {
                log_records("host job interrupted by the restart", &records);
            }
            Err(error) => {
                tracing::warn!(
                    error = %format!("{error:#}"),
                    "could not recover host jobs after the restart",
                );
            }
        }
        apply_pending(work_dir);
    }

    /// The graceful stop: every running job is cancelled with
    /// `daemon stopped` and its child killed.
    pub(super) fn stop_host_jobs(&mut self) {
        if let Some(runner) = self.host_jobs.as_mut() {
            runner.shutdown();
        }
    }
}

/// Build the runner `Orchestrator::new` could not. While none can be built,
/// every queued job fails with the reason, so no waiter reports it pending
/// forever.
fn retry_runner(work_dir: &Path) -> Option<HostJobRunner> {
    let error = match RunnerConfig::daemon_default() {
        Ok(config) => return Some(HostJobRunner::new(work_dir.to_path_buf(), config)),
        Err(error) => error,
    };
    let detail = format!("no verify-host runner: {error:#}");
    match fail_queued(work_dir, &detail) {
        Ok(failed) => {
            log_records("host job failed: no runner", &failed);
            for record in &failed {
                apply_record(work_dir, record);
            }
        }
        Err(error) => tracing::warn!(
            error = %format!("{error:#}"),
            "could not fail the queued host jobs no runner can start",
        ),
    }
    None
}

fn log_records(message: &str, records: &[JobRecord]) {
    for record in records {
        tracing::info!(
            stage = %record.stage_id,
            job = %record.id,
            state = ?record.state,
            "{message}",
        );
    }
}

#[cfg(test)]
#[path = "host_jobs_tests.rs"]
mod tests;
