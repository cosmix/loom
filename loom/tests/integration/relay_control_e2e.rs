//! End-to-end relay of a control request kind through the PostToolUse relay hook.
//!
//! `loom stage block` is a control kind: the real `loom-relay.sh` confirms it
//! with `LOOM relay: received block <id8>` when its relay line reaches the hook,
//! and never sweeps its ticket later when the line was dropped. A dropped
//! ticket stays on disk, `loom request status` reports it as pending relay, and
//! once it is more than a minute old it stops counting against the session's
//! unconsumed-ticket quota without being deleted.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime};

use loom::fs::session_files::save_session;
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::plan::schema::AcceptanceCriterion;
use loom::process::sandbox_probe::skip_unless;
use loom::relay::{RelayLine, RequestKind, MAX_UNCONSUMED_TICKETS};
use loom::verify::transitions::save_stage;
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

use super::helpers;
use super::relay_scratch::{create_scratch_dir, tempdir_outside_tmp};

const STAGE_ID: &str = "relay-control-e2e-stage";
const OLD: Duration = Duration::from_secs(120);

/// Everything one run needs, kept alive for the fixture's lifetime.
struct Fixture {
    _dirs: Vec<TempDir>,
    repo_root: PathBuf,
    work_dir: PathBuf,
    worktree_path: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
    scratch_dir: PathBuf,
    session_id: String,
}

/// What a session's `loom stage block` printed, and the line it ended on.
struct Blocked {
    line: RelayLine,
    output: Output,
}

/// A `<pid_key>.pid` file carrying the test process's own verified identity, so
/// a hook subprocess this test spawns proves as "inside the session" (the same
/// evidence `relay_e2e.rs` writes).
fn write_running_pid_entry(work_dir: &Path, tracking_key: &str, session_id: &str) {
    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid)
        .expect("the test process's own start time must be readable on this platform");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    fs::write(
        pids_dir.join(format!("{tracking_key}-{session_id}.pid")),
        format!("{pid}\n{start_time}\n"),
    )
    .expect("write pid entry");
}

fn write_session_and_stage(work_dir: &Path, worktree: &Path, session_id: &str) {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(STAGE_ID.to_string());
    session.tracking_key = Session::derive_tracking_key(STAGE_ID, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");
    write_running_pid_entry(work_dir, &session.tracking_key, session_id);

    let stage = Stage {
        id: STAGE_ID.to_string(),
        name: STAGE_ID.to_string(),
        status: StageStatus::Executing,
        session: Some(session_id.to_string()),
        acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
        ..Default::default()
    };
    save_stage(&stage, work_dir).expect("save stage record");
}

fn build_fixture() -> Fixture {
    let repo = helpers::init_test_repo();
    let repo_root = repo.path().to_path_buf();
    let work_dir = repo_root.join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");
    let worktree = TempDir::new().expect("create worktree dir");
    let home = tempdir_outside_tmp("home-");
    let xdg = tempdir_outside_tmp("xdg-");
    let session_id = format!(
        "relay-control-e2e-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    );
    write_session_and_stage(&work_dir, worktree.path(), &session_id);
    let scratch_dir = create_scratch_dir(xdg.path(), home.path(), &session_id);
    let (worktree_path, home_path, xdg_path) = (
        worktree.path().to_path_buf(),
        home.path().to_path_buf(),
        xdg.path().to_path_buf(),
    );
    Fixture {
        _dirs: vec![repo, worktree, home, xdg],
        repo_root,
        work_dir,
        worktree_path,
        home: home_path,
        xdg: xdg_path,
        scratch_dir,
        session_id,
    }
}

impl Fixture {
    /// A `loom` command (via `helpers::loom_cmd`) as the Stage session.
    fn session_loom(&self) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .current_dir(&self.worktree_path)
            .env("LOOM_SESSION_ID", &self.session_id)
            .env("LOOM_STAGE_ID", STAGE_ID)
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("LOOM_WORKTREE_PATH", &self.worktree_path)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home);
        command
    }

    fn ticket(&self, line: &RelayLine) -> PathBuf {
        self.scratch_dir.join(format!("{}.req", line.id))
    }

    /// `loom stage block`, as the session would run it.
    fn block(&self) -> Blocked {
        let output = self
            .session_loom()
            .args(["stage", "block", STAGE_ID, "relay control e2e"])
            .output()
            .expect("spawn loom stage block");
        assert!(
            output.status.success(),
            "loom stage block failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let last = stdout.lines().next_back().expect("stdout carries a line");
        let line = RelayLine::parse(last).expect("last stdout line is a relay line");
        assert_eq!(line.kind, RequestKind::Block);
        Blocked { line, output }
    }

    /// `loom request status <id>` as the session: (exit success, stdout and stderr).
    fn status(&self, line: &RelayLine) -> (bool, String) {
        // From the repository root: the plain-directory worktree here has no
        // `.loom/work` link for the command to find the state directory through.
        let output = self
            .session_loom()
            .current_dir(&self.repo_root)
            .args(["request", "status", &line.id])
            .output()
            .expect("spawn loom request status");
        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }

    /// Run the real `loom-relay.sh` as a direct child of this process, with the
    /// payload a Bash call would hand it for `command` and `stdout`.
    fn run_hook(&self, command: &str, stdout: &str, stderr: &str) -> Option<String> {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("loom-hooks")
            .join("loom-relay.sh");
        let path = std::env::var_os("PATH").unwrap_or_default();
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "tool_response": {"stdout": stdout, "stderr": stderr},
        })
        .to_string();
        let mut child = Command::new("bash")
            .arg(&hook)
            .env("LOOM_SESSION_ID", &self.session_id)
            .env("LOOM_STAGE_ID", STAGE_ID)
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home)
            .env("LOOM_BIN", helpers::loom_bin_path())
            .env("LOOM_HOOK_PATH", &path)
            .env("PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn loom-relay.sh");
        child
            .stdin
            .take()
            .expect("child stdin is piped")
            .write_all(payload.as_bytes())
            .expect("write hook payload");
        let output = child.wait_with_output().expect("wait for loom-relay.sh");
        assert!(output.status.success(), "loom-relay.sh must always exit 0");
        let text = String::from_utf8_lossy(&output.stdout);
        if text.trim().is_empty() {
            return None;
        }
        let reply: serde_json::Value = serde_json::from_str(text.trim()).expect("hook reply");
        reply["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .map(str::to_string)
    }
}

fn age(path: &Path) {
    fs::File::options()
        .write(true)
        .open(path)
        .expect("open ticket")
        .set_modified(SystemTime::now() - OLD)
        .expect("backdate ticket");
}

fn skip_without_tools(name: &str) -> bool {
    skip_unless(
        helpers::tool_available("jq") && helpers::tool_available("bash"),
        name,
        "jq or bash was not found on PATH",
    )
}

fn block_command() -> String {
    format!("loom stage block {STAGE_ID} \"relay control e2e\"")
}

/// Run bare, the relay line reaches the hook, which confirms the control kind
/// by name and short id, and the CLI's notice promises exactly that line.
#[test]
#[serial]
fn a_bare_block_is_confirmed_with_its_kind_and_short_id() {
    if skip_without_tools("a_bare_block_is_confirmed_with_its_kind_and_short_id") {
        return;
    }
    let fx = build_fixture();
    let blocked = fx.block();
    let id8 = &blocked.line.id[..8];

    let stderr = String::from_utf8_lossy(&blocked.output.stderr).into_owned();
    assert!(
        stderr.contains(&format!("`LOOM relay: received block {id8}`")),
        "the notice must name the confirmation the hook prints: {stderr}"
    );
    assert!(stderr.contains("NOT filed"), "{stderr}");

    let stdout = String::from_utf8_lossy(&blocked.output.stdout).into_owned();
    let reply = fx
        .run_hook(&block_command(), &stdout, &stderr)
        .expect("the hook must reply to a relayed block");
    assert!(
        reply.contains(&format!("received block {id8}")),
        "unexpected hook reply: {reply}"
    );
    assert!(
        !fx.ticket(&blocked.line).exists(),
        "a relayed ticket is consumed"
    );
}

/// With its relay line dropped the ticket stays and reads as pending relay; old
/// control tickets stop counting against the quota but none is deleted.
#[test]
#[serial]
fn a_dropped_control_ticket_stays_pending_and_stops_consuming_quota() {
    if skip_without_tools("a_dropped_control_ticket_stays_pending_and_stops_consuming_quota") {
        return;
    }
    let fx = build_fixture();
    let first = fx.block();

    // The incident spelling: the relay line filtered out of the call's output.
    let filtered = format!("{} 2>&1 | grep -v '^LOOM_RELAY'", block_command());
    assert!(fx.run_hook(&filtered, "", "").is_none());
    let ticket = fx.ticket(&first.line);
    assert!(ticket.is_file(), "a control ticket is never swept");
    let (ok, stdout) = fx.status(&first.line);
    assert!(ok, "{stdout}");
    assert!(stdout.contains("pending relay"), "{stdout}");

    age(&ticket);
    let mut tickets = vec![ticket];
    for _ in 1..MAX_UNCONSUMED_TICKETS {
        let next = fx.ticket(&fx.block().line);
        age(&next);
        tickets.push(next);
    }
    assert_eq!(tickets.len(), MAX_UNCONSUMED_TICKETS);

    // One more still emits: the old control tickets no longer fill the quota.
    let last = fx.block();
    assert!(
        tickets.iter().all(|path| path.is_file()),
        "no earlier ticket may be deleted"
    );
    let stdout = String::from_utf8_lossy(&last.output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&last.output.stderr).into_owned();
    let reply = fx
        .run_hook(&block_command(), &stdout, &stderr)
        .expect("the newest request is still relayed");
    assert!(reply.contains(&format!("received block {}", &last.line.id[..8])));
}
