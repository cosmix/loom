//! Host-verification specific contract helpers.
use crate::helpers;
use chrono::Utc;
use loom::host_verify::runner::{HostJobRunner, RunnerConfig};
use loom::host_verify::store::load_current;
use loom::host_verify::{new_job_id, JobKind, JobRecord, JobState};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
#[path = "../host_job_fixture/mod.rs"]
mod host_job_fixture;
pub use host_job_fixture::*;

/// A runner that spawns the real `loom stage verify-host` binary, with a
/// private `LOOM_HOME` whose update check is off. Keep the returned directory
/// and guard alive while the runner works.
pub fn verify_host_runner(work_dir: PathBuf) -> (HostJobRunner, TempDir, helpers::EnvVarGuard) {
    let loom_home = TempDir::new().unwrap();
    let config_toml = loom_home.path().join("config.toml");
    fs::write(config_toml, "[update]\ncheck = false\n").unwrap();
    let guard = helpers::EnvVarGuard::set("LOOM_HOME", loom_home.path());
    let config = RunnerConfig {
        program: env!("CARGO_BIN_EXE_loom").into(),
        args_prefix: vec!["stage".into(), "verify-host".into()],
        slots: 1,
    };
    (HostJobRunner::new(work_dir, config), loom_home, guard)
}

impl Fixture {
    /// `loom stage verify-host stage-a --job <id>` as the daemon would run
    /// it: no session variables, `LOOM_WORK_DIR` set, run from `cwd`.
    pub fn verify_host(&self, job_id: &str, cwd: &Path) -> Command {
        let stage = loom::verify::transitions::load_stage(STAGE, &self.work_dir).unwrap();
        let policy =
            loom::handoff::completion::policy::effective_policy_hash(&stage, &self.work_dir)
                .unwrap();
        let mut command = helpers::loom_cmd();
        command
            .args(["stage", "verify-host", STAGE, "--job", job_id])
            .current_dir(cwd)
            .env("LOOM_VERIFY_BUDGET_POLICY", policy)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("HOME", &self.home);
        isolate_git(&mut command, &self.home);
        command
    }
}

/// A Check record for `stage_id` by session-a, with a fresh job id.
pub fn record(stage_id: &str, state: JobState, head: &str, tree_digest: &str) -> JobRecord {
    let now = Utc::now();
    JobRecord {
        id: new_job_id(),
        submission_order: 0,
        stage_id: stage_id.to_string(),
        session_id: SESSION.to_string(),
        kind: JobKind::Check,
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
        no_cache: false,
        criteria: None,
        state,
        requests: vec!["r1".to_string()],
        queued_at: now,
        started_at: (state == JobState::Running).then_some(now),
        finished_at: None,
        pid: None,
        detail: None,
    }
}

/// True while `pid` names a process (a zombie counts).
pub fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the pid exists.
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    ret == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Poll `check` every 100 ms until it holds or `limit` passes.
pub fn wait_until(limit: Duration, mut check: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    check()
}

/// Tick until stage-a's current record is Running with a pid; returns it.
pub fn tick_until_running(runner: &mut HostJobRunner, work_dir: &Path) -> JobRecord {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        runner.tick().expect("runner tick");
        let current = load_current(work_dir, STAGE).expect("load current record");
        if let Some(record) = current.filter(|r| r.state == JobState::Running && r.pid.is_some()) {
            return record;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the job never reached Running with a pid within 20 s");
}
