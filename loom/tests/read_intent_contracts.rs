//! Contract: the read guard prepares no read-receipt intent.

#[path = "integration/helpers.rs"]
// only loom_bin_path() and clear_relay_env() are used
#[allow(dead_code)]
mod helpers;

use loom::fs::permissions::constants::{
    HOOK_BOUNDED, HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_GUARD, HOOK_READ_LEDGER,
};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn install_read_guard() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("create hook dir");
    for (name, content) in [
        ("_bounded.sh", HOOK_BOUNDED),
        ("_common.sh", HOOK_COMMON),
        ("_read_discipline.sh", HOOK_READ_DISCIPLINE),
        ("_read_ledger.sh", HOOK_READ_LEDGER),
        ("read-guard.sh", HOOK_READ_GUARD),
    ] {
        let path = temp.path().join(name);
        fs::write(&path, content).unwrap_or_else(|e| panic!("write {name}: {e}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let hook = temp.path().join("read-guard.sh");
    (temp, hook)
}

fn scratch_loom_home() -> TempDir {
    let home = TempDir::new().expect("create LOOM_HOME");
    fs::write(home.path().join("config.toml"), "[update]\ncheck = false\n")
        .expect("write scratch user config");
    home
}

fn run_read_guard(hook: &Path, work: &Path, loom_home: &Path, payload: &str) -> i32 {
    let mut cmd = Command::new("bash");
    helpers::clear_relay_env(&mut cmd)
        .arg(hook)
        .current_dir(work)
        .env("LOOM_BIN", helpers::loom_bin_path())
        .env("LOOM_STAGE_ID", "stage-1")
        .env("LOOM_SESSION_ID", "sess-1")
        .env("LOOM_WORK_DIR", work)
        .env("TMPDIR", work)
        .env("LOOM_HOME", loom_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn read-guard.sh");
    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(payload.as_bytes())
        .expect("write payload");
    let output = child.wait_with_output().expect("wait for read-guard.sh");
    assert!(
        output.status.code().is_some(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.status.code().unwrap_or(-1)
}

#[test]
fn read_guard_prepares_no_receipt_intent() {
    let (_hooks, hook) = install_read_guard();
    let work = TempDir::new().expect("create work dir");
    let loom_home = scratch_loom_home();
    let file = work.path().join("sample.rs");
    fs::write(&file, "fn line() {}\n".repeat(50)).expect("write 50-line file");
    let payload = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file},
        "agent_id": "agent-1",
        "session_id": "sess-1",
        "cwd": work.path(),
    })
    .to_string();

    let code = run_read_guard(&hook, work.path(), loom_home.path(), &payload);

    assert_eq!(code, 0, "read-guard.sh must allow the read");
    let receipts = work.path().join("hooks/reads/sess-1/receipts");
    assert!(
        !receipts.exists(),
        "read-guard.sh prepared a receipt intent under {}",
        receipts.display()
    );
    let fallback = work.path().join("loom-reads");
    assert!(
        !fallback.exists(),
        "read-guard.sh wrote receipt state under {}",
        fallback.display()
    );
}
