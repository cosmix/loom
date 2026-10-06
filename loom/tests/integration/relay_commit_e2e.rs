//! End-to-end relayed stage commit: `loom stage commit`, the real
//! `loom-relay.sh` hook, `Orchestrator::drain_session_inboxes`, then
//! `loom request status --wait`. The fixture mirrors `relay_e2e.rs` plus a
//! registered worktree at `.worktrees/<stage>`. No socket is opened.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use loom::fs::inbox::{read_ledger, LedgerOutcome};
use loom::fs::session_files::save_session;
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::schema::AcceptanceCriterion;
use loom::process::sandbox_probe::skip_unless;
use loom::relay::{RelayLine, RequestKind};
use loom::verify::transitions::save_stage;
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

use super::helpers;

const MESSAGE: &str = "test(e2e): commit a.txt";
const FAKE_SIGNER: &str = r#"#!/bin/sh
cat >/dev/null
fd=2
for arg in "$@"; do
  case "$arg" in --status-fd=*) fd="${arg#--status-fd=}" ;; esac
done
eval "printf '\n[GNUPG:] SIG_CREATED D 1 8 00 0 FAKE\n' >&$fd"
printf '%s\n' '-----BEGIN PGP SIGNATURE-----' 'ZmFrZSBzaWduYXR1cmUgZm9yIGxvb20gY29udHJhY3Q=' '-----END PGP SIGNATURE-----'
"#;

/// Everything one run needs; the temp dirs live as long as the fixture.
struct Fixture {
    _dirs: Vec<TempDir>,
    repo_root: PathBuf,
    work_dir: PathBuf,
    worktree_path: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
    scratch_dir: PathBuf,
    session_id: String,
    stage_id: String,
}

/// What the session saw before it relayed, and what its command printed.
struct Staged {
    head: String,
    tree: String,
    line: RelayLine,
    output: Output,
}

/// Point every git this test spawns at missing global and system config.
fn isolate_git(command: &mut Command, home: &Path) {
    command
        .env("GIT_CONFIG_GLOBAL", home.join("no-global-gitconfig"))
        .env("GIT_CONFIG_SYSTEM", home.join("no-system-gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
}

/// Run git in `dir` with ambient config neutralised; returns trimmed stdout.
fn git(home: &Path, dir: &Path, args: &[&str]) -> String {
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

/// A temp dir outside `/tmp`, which the scratch root refuses.
fn tempdir_outside_tmp() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/relay-e2e-tmp");
    fs::create_dir_all(&base).expect("create target/relay-e2e-tmp");
    let dir = tempfile::Builder::new()
        .prefix("xdg-")
        .tempdir_in(&base)
        .expect("create xdg tempdir outside /tmp");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).expect("chmod xdg dir");
    dir
}

/// A Running Stage session and an Executing stage it owns, with the pid entry
/// of the test process itself so a hook child proves as "inside the session".
fn write_session_and_stage(work_dir: &Path, worktree: &Path, session_id: &str, stage_id: &str) {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(stage_id.to_string());
    session.tracking_key = Session::derive_tracking_key(stage_id, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");

    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid).expect("own process start time");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    let pid_file = format!("{}-{session_id}.pid", session.tracking_key);
    fs::write(pids_dir.join(pid_file), format!("{pid}\n{start_time}\n")).expect("write pid entry");

    let stage = Stage {
        id: stage_id.to_string(),
        name: stage_id.to_string(),
        status: StageStatus::Executing,
        session: Some(session_id.to_string()),
        worktree: Some(stage_id.to_string()),
        acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
        ..Default::default()
    };
    save_stage(&stage, work_dir).expect("save stage record");
}

/// Isolated-git repo, config pinned repo-locally (drain reads host config), and a stage worktree.
fn init_repo_with_worktree(home: &Path, stage_id: &str) -> (TempDir, PathBuf) {
    let repo = TempDir::new().expect("create repo dir");
    let root = repo.path().to_path_buf();
    git(home, &root, &["init", "-b", "main"]);
    let hooks = root.join(".git").join("hooks");
    fs::create_dir_all(&hooks).expect("create hooks dir");
    for (key, value) in [
        ("user.email", "test@test.com"),
        ("user.name", "Test User"),
        ("commit.gpgsign", "false"),
        ("gpg.format", "openpgp"),
        ("core.hooksPath", hooks.to_str().unwrap()),
    ] {
        git(home, &root, &["config", key, value]);
    }
    git(home, &root, &["commit", "--allow-empty", "-m", "initial"]);
    let worktree = root.join(".worktrees").join(stage_id);
    let branch = format!("loom/{stage_id}");
    let path = worktree.to_str().unwrap();
    git(home, &root, &["worktree", "add", "-b", &branch, path]);
    (repo, worktree)
}

fn build_fixture() -> Fixture {
    let home = TempDir::new().expect("create home dir");
    let xdg = tempdir_outside_tmp();
    let stage_id = "relay-commit-e2e-stage".to_string();
    let (repo, worktree_path) = init_repo_with_worktree(home.path(), &stage_id);
    let repo_root = repo.path().to_path_buf();
    let work_dir = repo_root.join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");
    // A stage worktree reaches the shared state through this symlink.
    fs::create_dir_all(worktree_path.join(".loom")).expect("create worktree .loom dir");
    std::os::unix::fs::symlink(&work_dir, worktree_path.join(".loom/work"))
        .expect("link worktree .loom/work to the shared state");

    let uuid = Uuid::new_v4().simple();
    let session_id = format!("relay-commit-e2e-{}-{uuid}", std::process::id());
    write_session_and_stage(&work_dir, &worktree_path, &session_id, &stage_id);
    let scratch_dir = xdg.path().join("loom").join("scratch").join(&session_id);
    fs::create_dir_all(&scratch_dir).expect("create scratch dir");
    fs::set_permissions(&scratch_dir, fs::Permissions::from_mode(0o700)).expect("chmod scratch");

    let (home_path, xdg_path) = (home.path().to_path_buf(), xdg.path().to_path_buf());
    Fixture {
        _dirs: vec![repo, home, xdg],
        repo_root,
        work_dir,
        worktree_path,
        home: home_path,
        xdg: xdg_path,
        scratch_dir,
        session_id,
        stage_id,
    }
}

impl Fixture {
    /// Git in the main checkout (`in_worktree` false) or the stage worktree.
    fn git(&self, in_worktree: bool, args: &[&str]) -> String {
        let dir = if in_worktree {
            &self.worktree_path
        } else {
            &self.repo_root
        };
        git(&self.home, dir, args)
    }

    /// A `loom` command (via `helpers::loom_cmd`) as the Stage session.
    fn session_loom(&self) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .env_remove("LOOM_STAGE_ID")
            .env_remove("LOOM_SESSION_ID")
            .current_dir(&self.worktree_path)
            .env("LOOM_SESSION_ID", &self.session_id)
            .env("LOOM_STAGE_ID", &self.stage_id)
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("LOOM_WORKTREE_PATH", &self.worktree_path)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home);
        isolate_git(&mut command, &self.home);
        command
    }
}

/// Stage `a.txt`, record what the session saw, and run `loom stage commit`.
fn stage_and_request_commit(fx: &Fixture) -> Staged {
    fs::write(fx.worktree_path.join("a.txt"), "a\n").expect("write a.txt");
    fx.git(true, &["add", "a.txt"]);
    let head = fx.git(true, &["rev-parse", "HEAD"]);
    let tree = fx.git(true, &["write-tree"]);

    let mut command = fx.session_loom();
    command.args(["stage", "commit", &fx.stage_id, "-m", MESSAGE]);
    let output = command
        .stdin(Stdio::null())
        .output()
        .expect("spawn loom stage commit");
    assert!(
        output.status.success(),
        "loom stage commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_line = stdout.lines().next_back().expect("stdout carries a line");
    let line = RelayLine::parse(last_line).expect("last stdout line is a relay line");
    assert_eq!(line.kind, RequestKind::Commit);
    Staged {
        head,
        tree,
        line,
        output,
    }
}

/// Run the real `loom-relay.sh` as a direct child of THIS process, so its
/// `loom hook relay` grandchild sits inside the recorded session process tree.
fn run_hook(fx: &Fixture, staged: &Staged) -> Output {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": format!("loom stage commit {} -m \"{MESSAGE}\"", fx.stage_id)},
        "tool_response": {
            "stdout": String::from_utf8_lossy(&staged.output.stdout),
            "stderr": String::from_utf8_lossy(&staged.output.stderr),
        },
    })
    .to_string();
    let hook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/loom-relay.sh");
    let real_path = std::env::var_os("PATH").unwrap_or_default();

    let mut command = Command::new("bash");
    command
        .arg(&hook_path)
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_SESSION_ID")
        .env("LOOM_SESSION_ID", &fx.session_id)
        .env("LOOM_STAGE_ID", &fx.stage_id)
        .env("LOOM_SCRATCH_DIR", &fx.scratch_dir)
        .env("LOOM_WORK_DIR", &fx.work_dir)
        .env("XDG_RUNTIME_DIR", &fx.xdg)
        .env("HOME", &fx.home)
        .env("LOOM_BIN", helpers::loom_bin_path())
        .env("LOOM_HOOK_PATH", &real_path)
        .env("PATH", &real_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_git(&mut command, &fx.home);
    let mut child = command.spawn().expect("spawn loom-relay.sh");
    let stdin = child.stdin.take().expect("child stdin is piped");
    { stdin }
        .write_all(payload.as_bytes())
        .expect("write hook payload");
    child.wait_with_output().expect("wait for loom-relay.sh")
}

fn drain_once(fx: &Fixture) {
    let graph = helpers::build_test_graph(vec![(fx.stage_id.as_str(), vec![])]);
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root: fx.repo_root.clone(),
        manual_mode: true,
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator = Orchestrator::new(config, graph).expect("construct orchestrator");
    orchestrator.drain_session_inboxes();
}

/// `loom request status <id> --wait 5` as the session; the commit id it
/// prints (`<id>: applied: committed <commit>`).
fn wait_for_commit_id(fx: &Fixture, request_id: &str) -> String {
    let mut command = fx.session_loom();
    command.args(["request", "status", request_id, "--wait", "5"]);
    let output = command.output().expect("spawn loom request status");
    assert!(
        output.status.success(),
        "loom request status --wait failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let (_, printed) = stdout
        .trim()
        .rsplit_once("committed ")
        .unwrap_or_else(|| panic!("status output names no commit: {stdout}"));
    printed.trim().to_string()
}

/// Relay the request through the hook, drain once, wait for the outcome, and
/// assert the commit landed exactly as staged. Returns the new commit id.
fn relay_and_assert_landed(fx: &Fixture, staged: &Staged) -> String {
    let hook_output = run_hook(fx, staged);
    assert!(
        hook_output.status.success(),
        "loom-relay.sh must always exit 0: stderr={}",
        String::from_utf8_lossy(&hook_output.stderr)
    );
    drain_once(fx);

    let id = &staged.line.id;
    let ledger = read_ledger(&fx.work_dir, &fx.session_id).expect("read ledger");
    let last = ledger.iter().rev().find(|record| &record.id == id);
    let last = last.unwrap_or_else(|| panic!("ledger has no row for request {id}"));
    assert_eq!(last.outcome, Some(LedgerOutcome::Applied), "{last:?}");

    let printed = wait_for_commit_id(fx, id);
    let tip = fx.git(
        false,
        &["rev-parse", &format!("refs/heads/loom/{}", fx.stage_id)],
    );
    assert_eq!(printed, tip);
    assert_eq!(
        fx.git(false, &["rev-parse", &format!("{printed}^{{tree}}")]),
        staged.tree
    );
    let parents = fx.git(false, &["rev-list", "--parents", "-n", "1", &printed]);
    assert_eq!(parents, format!("{printed} {}", staged.head));
    printed
}

/// Turn signing on repo-locally with a fake `gpg.program` kept in `dir`.
fn enable_fake_signer(fx: &Fixture, dir: &Path) {
    let script = dir.join("fake-gpg");
    fs::write(&script, FAKE_SIGNER).expect("write fake signer");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod fake signer");
    fx.git(false, &["config", "gpg.program", script.to_str().unwrap()]);
    fx.git(false, &["config", "commit.gpgsign", "true"]);
}

/// True (after reporting the skip) when a tool this flow needs is missing.
fn skip_for_missing_tools(test_name: &str) -> bool {
    let present = ["jq", "bash", "git"]
        .iter()
        .all(|tool| helpers::tool_available(tool));
    skip_unless(present, test_name, "jq, bash or git was not found on PATH")
}

/// The relayed commit lands the staged tree on `loom/<stage>`.
#[test]
#[serial]
fn a_relayed_stage_commit_flows_end_to_end() {
    if skip_for_missing_tools("a_relayed_stage_commit_flows_end_to_end") {
        return;
    }
    // Orchestrator::new eagerly constructs a NativeBackend even in manual mode.
    let _terminal_env = helpers::EnvVarGuard::set("LOOM_TERMINAL", "xterm");

    let fx = build_fixture();
    let staged = stage_and_request_commit(&fx);
    relay_and_assert_landed(&fx, &staged);
}

/// With `commit.gpgsign` on, the daemon's commit carries a `gpgsig` header.
#[test]
#[serial]
fn a_relayed_commit_is_signed_when_gpgsign() {
    if skip_for_missing_tools("a_relayed_commit_is_signed_when_gpgsign") {
        return;
    }
    let _terminal_env = helpers::EnvVarGuard::set("LOOM_TERMINAL", "xterm");

    let fx = build_fixture();
    let signer_dir = TempDir::new().expect("create signer dir");
    enable_fake_signer(&fx, signer_dir.path());
    let staged = stage_and_request_commit(&fx);
    let id = relay_and_assert_landed(&fx, &staged);

    let object = fx.git(false, &["cat-file", "commit", &id]);
    assert!(
        object.lines().any(|line| line.starts_with("gpgsig ")),
        "commit {id} carries no gpgsig header:\n{object}"
    );
}
