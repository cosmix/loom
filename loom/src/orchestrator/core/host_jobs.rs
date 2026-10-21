//! The host-job runner on the orchestrator tick (common.md 3.4): a pass of
//! the runner after the session inboxes drain, recovery once at startup, and
//! a graceful stop when the run ends.

use std::path::Path;

use anyhow::Result;

use super::Orchestrator;
use crate::host_verify::runner::{HostJobRunner, RunnerConfig};
use crate::host_verify::JobRecord;

/// The runner the daemon starts with: the running loom binary's
/// `stage verify-host`. `None`, logged, when that binary cannot be located:
/// host jobs then stay disabled for this run instead of stopping it.
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
    /// One pass of the host-job runner: end, reap and start jobs. A runner
    /// error is logged and never propagated: a broken job must not stop the
    /// orchestrator.
    pub fn tick_host_jobs(&mut self) {
        let Some(runner) = self.host_jobs.as_mut() else {
            return;
        };
        match runner.tick() {
            Ok(ended) => log_records("host job ended", &ended),
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), "host job tick failed");
            }
        }
    }

    /// Replace the runner `Orchestrator::new` built; the replaced one is
    /// stopped first.
    pub fn set_host_job_runner(&mut self, runner: HostJobRunner) {
        if let Some(mut replaced) = self.host_jobs.replace(runner) {
            replaced.shutdown();
        }
    }

    /// Log what `recover_after_restart` changed at startup.
    pub(super) fn report_host_job_recovery(&self, recovered: Result<Vec<JobRecord>>) {
        match recovered {
            Ok(records) => log_records("host job interrupted by the restart", &records),
            Err(error) => tracing::warn!(
                error = %format!("{error:#}"),
                "could not recover host jobs after the restart",
            ),
        }
    }

    /// The graceful stop: every running job is cancelled with
    /// `daemon stopped` and its child killed.
    pub(super) fn stop_host_jobs(&mut self) {
        if let Some(runner) = self.host_jobs.as_mut() {
            runner.shutdown();
        }
    }
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
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use chrono::Utc;
    use tempfile::TempDir;

    use super::super::{Orchestrator, OrchestratorConfig};
    use crate::fs::work_dir::write_terminal_config;
    use crate::host_verify::runner::{HostJobRunner, RunnerConfig};
    use crate::host_verify::store::load_current;
    use crate::host_verify::submit::{submit, Submission, SubmitRequest};
    use crate::host_verify::{JobKind, JobRecord, JobState};
    use crate::models::session::{SessionBackendKind, TerminalConfig};
    use crate::models::stage::{Stage, StageStatus};
    use crate::plan::ExecutionGraph;

    const STAGE: &str = "stage-a";
    /// Writes a passing `result.json` for the job the runner named.
    const PASS: &str = r#"printf '{"job_id":"%s","head":"h","passed":true,"criteria":[],"error":null}' "$3" > "$LOOM_WORK_DIR/verification/$1/$3/result.json""#;

    /// An orchestrator over `<temp>/.loom/work` on the tmux lane (no terminal
    /// detection), with stage-a's file in place for the runner.
    fn orchestrator() -> (TempDir, PathBuf, Orchestrator) {
        let temp = TempDir::new().unwrap();
        let work = temp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        let tmux = TerminalConfig {
            backend: SessionBackendKind::Tmux,
        };
        write_terminal_config(&work, &tmux).unwrap();
        let stage = Stage {
            id: STAGE.to_string(),
            name: STAGE.to_string(),
            status: StageStatus::Executing,
            ..Stage::default()
        };
        crate::verify::transitions::save_stage(&stage, &work).unwrap();
        let config = OrchestratorConfig {
            work_dir: work.clone(),
            repo_root: temp.path().to_path_buf(),
            enable_skill_routing: false,
            ..Default::default()
        };
        let graph = ExecutionGraph::build(Vec::new()).unwrap();
        let orchestrator = Orchestrator::new(config, graph).unwrap();
        (temp, work, orchestrator)
    }

    fn shell_runner(work: &Path, script: &str) -> HostJobRunner {
        let config = RunnerConfig {
            program: PathBuf::from("/bin/sh"),
            args_prefix: vec!["-c".into(), script.into(), "sh".into()],
            slots: 1,
        };
        HostJobRunner::new(work.to_path_buf(), config)
    }

    fn queue_check(work: &Path) -> JobRecord {
        let request = SubmitRequest {
            stage_id: STAGE,
            session_id: "session-a",
            request_id: "r1",
            kind: JobKind::Check,
            head: "aaaa",
            tree_digest: "bbbb",
            no_cache: false,
        };
        match submit(work, &request, Utc::now()).unwrap() {
            Submission::Queued(record) => record,
            other => panic!("expected a queued job, got {other:?}"),
        }
    }

    /// Tick until the current record satisfies `done`, at most 20 s.
    fn tick_until(
        orchestrator: &mut Orchestrator,
        work: &Path,
        done: impl Fn(&JobRecord) -> bool,
    ) -> JobRecord {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(20) {
            orchestrator.tick_host_jobs();
            if let Some(record) = load_current(work, STAGE).unwrap().filter(&done) {
                return record;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the host job never reached the expected state");
    }

    #[test]
    fn the_tick_runs_a_queued_job_to_its_result() {
        let (_temp, work, mut orchestrator) = orchestrator();
        orchestrator.set_host_job_runner(shell_runner(&work, PASS));
        let job = queue_check(&work);

        let ended = tick_until(&mut orchestrator, &work, |r| r.state.is_terminal());

        assert_eq!(ended.id, job.id);
        assert_eq!(ended.state, JobState::Passed, "{ended:?}");
    }

    #[test]
    fn stopping_the_host_jobs_cancels_a_running_job_and_kills_its_child() {
        let (_temp, work, mut orchestrator) = orchestrator();
        orchestrator.set_host_job_runner(shell_runner(&work, "sleep 600"));
        queue_check(&work);
        let running = tick_until(&mut orchestrator, &work, |r| {
            r.state == JobState::Running && r.pid.is_some()
        });

        orchestrator.stop_host_jobs();

        let stopped = load_current(&work, STAGE).unwrap().unwrap();
        assert_eq!(stopped.state, JobState::Cancelled);
        assert_eq!(stopped.detail.as_deref(), Some("daemon stopped"));
        let pid = nix::unistd::Pid::from_raw(i32::try_from(running.pid.unwrap()).unwrap());
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH),
            "the stopped job's child must be killed and reaped"
        );
    }
}
