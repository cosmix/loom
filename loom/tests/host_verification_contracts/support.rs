//! Fixture for the host-verification contracts: a repository with one commit,
//! a stage worktree `.worktrees/stage-a`, its state root `<repo>/.loom/work`
//! holding stage-a and a running stage session, a plan the state root's
//! `config.toml` names, and a relay scratch directory. Shaped after
//! `tests/integration/relay_commit_e2e.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use chrono::Utc;
use loom::fs::session_files::save_session;
use loom::host_verify::runner::{HostJobRunner, RunnerConfig};
use loom::host_verify::store::load_current;
use loom::host_verify::{new_job_id, JobKind, JobRecord, JobState};
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::plan::schema::AcceptanceCriterion;
use loom::verify::transitions::save_stage;
use tempfile::TempDir;

use crate::helpers;

#[path = "../integration/relay_scratch.rs"]
mod relay_scratch;

pub const STAGE: &str = "stage-a";
pub const SESSION: &str = "session-a";

/// Everything one scenario needs; the temp dirs live as long as the fixture.
pub struct Fixture {
    _dirs: Vec<TempDir>,
    pub work_dir: PathBuf,
    pub worktree: PathBuf,
    pub home: PathBuf,
    pub xdg: PathBuf,
    pub scratch_dir: PathBuf,
}

/// Point a command's git at missing global and system config.
pub fn isolate_git(command: &mut Command, home: &Path) {
    command
        .env("GIT_CONFIG_GLOBAL", home.join("no-global-gitconfig"))
        .env("GIT_CONFIG_SYSTEM", home.join("no-system-gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
}

/// Run git in `dir` with ambient config neutralised; returns trimmed stdout.
pub fn git(home: &Path, dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.args(args).current_dir(dir);
    isolate_git(&mut command, home);
    let output = command.output().expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository whose one commit ignores `.loom/`, and the stage worktree.
fn init_repo_with_worktree(home: &Path) -> (TempDir, PathBuf) {
    let repo = TempDir::new().expect("create repo dir");
    let root = repo.path().to_path_buf();
    git(home, &root, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.email", "test@test.com"),
        ("user.name", "Test User"),
        ("commit.gpgsign", "false"),
    ] {
        git(home, &root, &["config", key, value]);
    }
    fs::write(root.join(".gitignore"), ".loom/\n").expect("write .gitignore");
    git(home, &root, &["add", ".gitignore"]);
    git(home, &root, &["commit", "-m", "initial"]);
    let worktree = root.join(".worktrees").join(STAGE);
    let branch = format!("loom/{STAGE}");
    git(
        home,
        &root,
        &["worktree", "add", "-b", &branch, worktree.to_str().unwrap()],
    );
    (repo, worktree)
}

/// stage-a: executing, plan version 1, owned by session-a, worktree stage-a.
pub fn write_stage(work_dir: &Path, acceptance: &[&str]) {
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::Executing,
        session: Some(SESSION.to_string()),
        worktree: Some(STAGE.to_string()),
        working_dir: Some(".".to_string()),
        plan_version: 1,
        acceptance: acceptance
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect(),
        ..Default::default()
    };
    save_stage(&stage, work_dir).expect("save stage record");
}

/// A running stage session-a whose worktree is the stage worktree.
fn write_session(work_dir: &Path, worktree: &Path) {
    let mut session = Session::new();
    session.id = SESSION.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(STAGE.to_string());
    session.tracking_key = Session::derive_tracking_key(STAGE, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");

    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid).expect("own process start time");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    let pid_file = format!("{}-{SESSION}.pid", session.tracking_key);
    fs::write(pids_dir.join(pid_file), format!("{pid}\n{start_time}\n")).expect("write pid entry");
}

/// A plan defining stage-a with `acceptance` and no goal checks, named by
/// `<work_dir>/config.toml`.
fn write_plan(work_dir: &Path, acceptance: &[&str]) {
    let plan = work_dir.join("PLAN-host.md");
    let mut yaml = format!(
        "loom:\n  version: 1\n  stages:\n    - id: {STAGE}\n      name: {STAGE}\n      working_dir: .\n      acceptance:\n"
    );
    for command in acceptance {
        let quoted = serde_json::to_string(command).expect("quote criterion");
        yaml.push_str(&format!("        - {quoted}\n"));
    }
    let body = format!(
        "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n<!-- END loom METADATA -->\n"
    );
    fs::write(&plan, body).expect("write plan");
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"host\"\nplan_name = \"host\"\nbase_branch = \"main\"\n",
        plan.display()
    );
    fs::write(work_dir.join("config.toml"), config).expect("write config.toml");
}

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

pub fn build_fixture(acceptance: &[&str]) -> Fixture {
    let home = relay_scratch::tempdir_outside_tmp("home-");
    let xdg = relay_scratch::tempdir_outside_tmp("xdg-");
    let (repo, worktree) = init_repo_with_worktree(home.path());
    let work_dir = repo.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");
    fs::create_dir_all(worktree.join(".loom")).expect("create worktree .loom dir");
    std::os::unix::fs::symlink(&work_dir, worktree.join(".loom/work"))
        .expect("link worktree .loom/work to the shared state");
    write_stage(&work_dir, acceptance);
    write_session(&work_dir, &worktree);
    write_plan(&work_dir, acceptance);
    let scratch_dir = relay_scratch::create_scratch_dir(xdg.path(), home.path(), SESSION);
    let (home_path, xdg_path) = (home.path().to_path_buf(), xdg.path().to_path_buf());
    Fixture {
        _dirs: vec![repo, home, xdg],
        work_dir,
        worktree,
        home: home_path,
        xdg: xdg_path,
        scratch_dir,
    }
}

impl Fixture {
    /// A `loom` command (via `helpers::loom_cmd`) as the stage session.
    pub fn session_loom(&self) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .current_dir(&self.worktree)
            .env("LOOM_SESSION_ID", SESSION)
            .env("LOOM_STAGE_ID", STAGE)
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("LOOM_WORKTREE_PATH", &self.worktree)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home);
        isolate_git(&mut command, &self.home);
        command
    }

    /// `loom stage verify-host stage-a --job <id>` as the daemon would run
    /// it: no session variables, `LOOM_WORK_DIR` set, run from `cwd`.
    pub fn verify_host(&self, job_id: &str, cwd: &Path) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .args(["stage", "verify-host", STAGE, "--job", job_id])
            .current_dir(cwd)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("HOME", &self.home);
        isolate_git(&mut command, &self.home);
        command
    }

    pub fn head(&self) -> String {
        git(&self.home, &self.worktree, &["rev-parse", "HEAD"])
    }
}

/// A bare state root `<tmp>/.loom/work`.
pub fn state_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create state root parent");
    let work_dir = dir.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create state root");
    (dir, work_dir)
}

/// A Check record for `stage_id` by session-a, with a fresh job id.
pub fn record(stage_id: &str, state: JobState, head: &str, tree_digest: &str) -> JobRecord {
    let now = Utc::now();
    JobRecord {
        id: new_job_id(),
        stage_id: stage_id.to_string(),
        session_id: SESSION.to_string(),
        kind: JobKind::Check,
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
        no_cache: false,
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
