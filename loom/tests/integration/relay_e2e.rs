//! End-to-end coverage of the relay chain (`doc/plans/PLAN-loom-state-confinement.md`
//! section 16, "Relay e2e"): a sandboxed `loom memory note` writes a ticket
//! and a `LOOM_RELAY_V1` line, the real `loom-relay.sh` PostToolUse hook
//! proves the caller sits inside the session's own process tree and hands the
//! request to the daemon inbox, and `Orchestrator::drain_session_inboxes`
//! applies it into the stage's memory journal — exactly once, even if the
//! captured hook payload is replayed.
//!
//! No `loom` command runs here (see the module doc on `helpers::loom_cmd`):
//! this test spawns the real binary for the CLI half and the real hook
//! script for the trust boundary, but drives the daemon half in-process.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use loom::fs::inbox::{read_ledger, LedgerOutcome, LedgerRecord};
use loom::fs::memory::read_journal;
use loom::fs::session_files::save_session;
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::schema::AcceptanceCriterion;
use loom::process::sandbox_probe::skip_unless;
use loom::relay::{RelayLine, RequestKind};
use loom::verify::transitions::save_stage;
use tempfile::TempDir;
use uuid::Uuid;

use super::helpers;

const TEST_NAME: &str = "relay_memory_note_flows_end_to_end";
const NOTE_TEXT: &str = "relay e2e probe";

/// Everything one run needs, kept alive for the fixture's lifetime.
struct Fixture {
    _repo: TempDir,
    _worktree: TempDir,
    _home: TempDir,
    _xdg: TempDir,
    repo_root: PathBuf,
    work_dir: PathBuf,
    worktree_path: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
    scratch_dir: PathBuf,
    session_id: String,
    stage_id: String,
}

/// A fresh temp dir that does NOT resolve under `/tmp` — required for the
/// scratch root: `relay::scratch::scratch_root` refuses one that does (real
/// sandboxes mount `/tmp` world-writable, so a root there would let any
/// sandboxed process on the host plant tickets), and this suite's own
/// `$TMPDIR` lives under `/tmp` (`relay/scratch.rs`'s own tests hit the same
/// constraint). `target/` is gitignored scratch space that is not.
fn tempdir_outside_tmp() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("relay-e2e-tmp");
    fs::create_dir_all(&base).expect("create target/relay-e2e-tmp");
    tempfile::Builder::new()
        .prefix("xdg-")
        .tempdir_in(&base)
        .expect("create xdg tempdir outside /tmp")
}

fn tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// `<xdg>/loom/scratch/<session_id>`, mode 0700 — the exact path
/// `scratch_root_from_env`/`session_dir` derive when `XDG_RUNTIME_DIR` is a
/// valid owned 0700 directory (`relay::scratch::linux_scratch_root`).
fn create_scratch_dir(xdg: &Path, session_id: &str) -> PathBuf {
    let scratch = xdg.join("loom").join("scratch").join(session_id);
    fs::create_dir_all(&scratch).expect("create scratch dir");
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).expect("chmod scratch dir");
    scratch
}

/// A `<pid_key>.pid` file carrying the test process's own verified identity —
/// the same evidence `caller_is_inside_session` requires
/// (`daemon/server/peer_identity.rs`). `window_title_and_pid_key`'s pid-key
/// formula (`tracking_key` non-empty) is re-derived here since that helper is
/// `pub(crate)` and unreachable from an integration test.
fn write_running_pid_entry(work_dir: &Path, tracking_key: &str, session_id: &str) {
    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid)
        .expect("the test process's own start time must be readable on this platform");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    let pid_key = format!("{tracking_key}-{session_id}");
    fs::write(
        pids_dir.join(format!("{pid_key}.pid")),
        format!("{pid}\n{start_time}\n"),
    )
    .expect("write pid entry");
}

/// A Running Stage session record whose pid evidence is the test process
/// itself, so a hook subprocess this test spawns proves as "inside the
/// session".
fn write_running_stage_session(work_dir: &Path, worktree: &Path, session_id: &str, stage_id: &str) {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(stage_id.to_string());
    session.tracking_key = Session::derive_tracking_key(stage_id, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");
    write_running_pid_entry(work_dir, &session.tracking_key, session_id);
}

/// An Executing stage owned by `session_id` — the state the drain's
/// attribution check and the hook's own `prove_session` check both accept.
fn write_executing_stage(work_dir: &Path, stage_id: &str, session_id: &str) {
    let stage = Stage {
        id: stage_id.to_string(),
        name: stage_id.to_string(),
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
    let home = TempDir::new().expect("create home dir");
    let xdg = tempdir_outside_tmp();
    fs::set_permissions(xdg.path(), fs::Permissions::from_mode(0o700)).expect("chmod xdg dir");

    let session_id = format!(
        "relay-e2e-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    );
    let stage_id = "relay-e2e-stage".to_string();

    write_running_stage_session(&work_dir, worktree.path(), &session_id, &stage_id);
    write_executing_stage(&work_dir, &stage_id, &session_id);
    let scratch_dir = create_scratch_dir(xdg.path(), &session_id);

    let worktree_path = worktree.path().to_path_buf();
    let home_path = home.path().to_path_buf();
    let xdg_path = xdg.path().to_path_buf();
    Fixture {
        _repo: repo,
        _worktree: worktree,
        _home: home,
        _xdg: xdg,
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

/// Run `loom memory note` as the Stage session would, over the real binary
/// (`helpers::loom_cmd`, so `binary_spawn_guard` and the `LOOM_HOME` scrub
/// both apply). Returns the parsed relay line plus the captured output the
/// hook will be fed next.
fn run_memory_note(fx: &Fixture) -> (RelayLine, Output) {
    let mut cmd = helpers::loom_cmd();
    cmd.arg("memory")
        .arg("note")
        .arg(NOTE_TEXT)
        .current_dir(&fx.worktree_path)
        .env("LOOM_SESSION_ID", &fx.session_id)
        .env("LOOM_STAGE_ID", &fx.stage_id)
        .env("LOOM_SESSION_TYPE", "stage")
        .env("LOOM_SCRATCH_DIR", &fx.scratch_dir)
        .env("LOOM_WORK_DIR", &fx.work_dir)
        .env("LOOM_WORKTREE_PATH", &fx.worktree_path)
        .env("XDG_RUNTIME_DIR", &fx.xdg)
        .env("HOME", &fx.home);
    let output = cmd.output().expect("spawn loom memory note");
    assert!(
        output.status.success(),
        "loom memory note failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_line = stdout.lines().next_back().expect("stdout carries a line");
    let line = RelayLine::parse(last_line).expect("last stdout line is a relay line");
    assert_eq!(line.kind, RequestKind::Memory);
    (line, output)
}

/// The PostToolUse JSON payload a real Bash tool call would hand the hook:
/// same shape as `loom-hooks/tests/loom-relay-kinds.sh`'s fixtures.
fn hook_payload(stdout: &str, stderr: &str) -> String {
    serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": format!("loom memory note \"{NOTE_TEXT}\"")},
        "tool_response": {"stdout": stdout, "stderr": stderr},
    })
    .to_string()
}

/// Run the real `loom-hooks/loom-relay.sh` as a direct child of THIS process
/// (never the test process's own env — only the child's), so the `loom hook
/// relay` grandchild it spawns lies inside the session's process tree that
/// `write_running_pid_entry` recorded.
fn run_hook(fx: &Fixture, payload: &str) -> Output {
    let hook_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("loom-hooks")
        .join("loom-relay.sh");
    let real_path = std::env::var_os("PATH").unwrap_or_default();

    let mut child = Command::new("bash")
        .arg(&hook_path)
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
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn loom-relay.sh");
    child
        .stdin
        .take()
        .expect("child stdin is piped")
        .write_all(payload.as_bytes())
        .expect("write hook payload");
    child.wait_with_output().expect("wait for loom-relay.sh")
}

/// The hook's `additionalContext` text, when it printed one.
fn hook_reply_text(output: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value =
        serde_json::from_str(trimmed).expect("hook reply is a JSON object");
    value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .map(str::to_string)
}

fn assert_inbox_entry_count(fx: &Fixture, expected: usize) {
    let dir = fx.work_dir.join("inbox").join(&fx.session_id);
    let count = fs::read_dir(&dir)
        .map(|read_dir| {
            read_dir
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        count, expected,
        "unexpected inbox entry count under {dir:?}"
    );
}

/// One id has two ledger lines: `applying` (outcome `None`) then the settled
/// outcome (`session_pass.rs::settle`) — the settled one is what matters.
fn assert_applied(records: &[LedgerRecord], id: &str) {
    let record = records
        .iter()
        .find(|record| record.id == id && record.outcome.is_some())
        .unwrap_or_else(|| panic!("ledger has no settled record for request {id}"));
    assert_eq!(record.outcome, Some(LedgerOutcome::Applied));
}

fn new_orchestrator(fx: &Fixture) -> Orchestrator {
    let graph = helpers::build_test_graph(vec![(fx.stage_id.as_str(), vec![])]);
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root: fx.repo_root.clone(),
        manual_mode: true,
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph).expect("construct orchestrator")
}

/// Step 4: the real hook proves the session and writes the inbox entry.
fn relay_via_hook(fx: &Fixture, line: &RelayLine, payload: &str) {
    let hook_output = run_hook(fx, payload);
    assert!(
        hook_output.status.success(),
        "loom-relay.sh must always exit 0: stderr={}",
        String::from_utf8_lossy(&hook_output.stderr)
    );
    let reply = hook_reply_text(&hook_output).expect("hook must report the relayed request");
    assert!(reply.contains("received"), "unexpected hook reply: {reply}");
    assert_inbox_entry_count(fx, 1);
    assert!(
        !fx.scratch_dir.join(format!("{}.req", line.id)).exists(),
        "the hook consumes the ticket once it is relayed"
    );
}

/// Step 5: drain exactly the pass the daemon's poll tick runs, and check the
/// note landed with an `applied` outcome.
fn drain_and_assert_applied(
    fx: &Fixture,
    orchestrator: &mut Orchestrator,
    line: &RelayLine,
) -> Vec<LedgerRecord> {
    orchestrator.drain_session_inboxes();
    let journal = read_journal(&fx.work_dir, &fx.stage_id).unwrap();
    assert_eq!(journal.entries.len(), 1);
    assert_eq!(journal.entries[0].content, NOTE_TEXT);
    let ledger = read_ledger(&fx.work_dir, &fx.session_id).expect("read ledger");
    assert_applied(&ledger, &line.id);
    assert_inbox_entry_count(fx, 0);
    ledger
}

/// Step 6: replaying the identical captured payload changes nothing. The
/// ticket is already gone, so the hook itself relays nothing new.
fn assert_replay_is_a_noop(
    fx: &Fixture,
    orchestrator: &mut Orchestrator,
    payload: &str,
    ledger_before: &[LedgerRecord],
) {
    let replay_output = run_hook(fx, payload);
    assert!(replay_output.status.success());
    assert!(
        hook_reply_text(&replay_output).is_none(),
        "a replayed payload must not be relayed a second time"
    );
    orchestrator.drain_session_inboxes();
    let journal = read_journal(&fx.work_dir, &fx.stage_id).unwrap();
    assert_eq!(journal.entries.len(), 1);
    let ledger_after = read_ledger(&fx.work_dir, &fx.session_id).expect("read ledger");
    assert_eq!(ledger_before, ledger_after);
    assert_inbox_entry_count(fx, 0);
}

/// The gate test for plan section 16: `loom memory note` (relay mode) ->
/// `loom-relay.sh` (real hook, real ancestry proof) -> daemon inbox ->
/// `drain_session_inboxes` (real handler), then a byte-identical replay of
/// the captured hook payload must change nothing.
#[test]
fn relay_memory_note_flows_end_to_end() {
    if skip_unless(
        tool_available("jq") && tool_available("bash"),
        TEST_NAME,
        "jq or bash was not found on PATH",
    ) {
        return;
    }

    let fx = build_fixture();

    // Step 3: the CLI ticket, with nothing applied yet.
    let (line, note_output) = run_memory_note(&fx);
    assert!(
        read_journal(&fx.work_dir, &fx.stage_id)
            .unwrap()
            .entries
            .is_empty(),
        "nothing should be applied before the hook ever runs"
    );

    let stdout = String::from_utf8_lossy(&note_output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&note_output.stderr).into_owned();
    let payload = hook_payload(&stdout, &stderr);
    relay_via_hook(&fx, &line, &payload);

    let mut orchestrator = new_orchestrator(&fx);
    let ledger_after_5 = drain_and_assert_applied(&fx, &mut orchestrator, &line);
    assert_replay_is_a_noop(&fx, &mut orchestrator, &payload, &ledger_after_5);
}
