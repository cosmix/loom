//! Contracts for stage platform-portability: the daemon socket path is measured
//! after symlink resolution, a daemon that aborts after signalling ready is a
//! launch failure, the boot ID prefers the session variable, and the lifecycle
//! hook resolves a start row when `wc` pads its count as BSD `wc` does.
//!
//! The surface is `loom::daemon::{socket_path, socket_path_problem, SOCKET_FILE,
//! SUN_PATH_MAX, await_ready, ReadyTiming}`,
//! `loom::process::boot_id::{BOOT_ID_ENV, resolve_boot_id}` and
//! `loom-hooks/_lifecycle.sh`'s `loom_lifecycle_resolve_start`. No contract
//! binds or dials a unix socket.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use loom::daemon::{
    await_ready, socket_path, socket_path_problem, ReadyTiming, SOCKET_FILE, SUN_PATH_MAX,
};
use loom::process::boot_id::{resolve_boot_id, BOOT_ID_ENV};
use tempfile::TempDir;

/// A temp dir and its canonical spelling (macOS `/var` is a symlink).
fn canonical_temp() -> (TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("pp")
        .tempdir()
        .expect("create temp dir");
    let root = fs::canonicalize(dir.path()).expect("canonicalize temp dir");
    (dir, root)
}

/// Creates `root/.loom/work` and a worktree spelling of it under an 80-character
/// stage id whose `.loom/work` is a symlink to the real directory. Returns the
/// real directory and the symlinked spelling.
fn symlinked_worktree(root: &Path) -> (PathBuf, PathBuf) {
    let real = root.join(".loom").join("work");
    fs::create_dir_all(&real).expect("create real work dir");
    let worktree_loom = root.join(".worktrees").join("a".repeat(80)).join(".loom");
    fs::create_dir_all(&worktree_loom).expect("create worktree .loom");
    let spelling = worktree_loom.join("work");
    symlink(&real, &spelling).expect("symlink worktree work dir");
    (real, spelling)
}

#[test]
fn socket_path_resolves_the_worktree_spelling() {
    let (_dir, root) = canonical_temp();
    let (real, spelling) = symlinked_worktree(&root);
    assert_eq!(SOCKET_FILE, "orchestrator.sock");

    let resolved = socket_path(&spelling);

    assert_eq!(
        resolved,
        real.join("orchestrator.sock"),
        "socket_path must resolve the symlinked worktree spelling {} to the real work dir",
        spelling.display()
    );
    assert!(
        spelling.join(SOCKET_FILE).as_os_str().len() > SUN_PATH_MAX,
        "the unresolved spelling must exceed the sun_path limit for this scenario to bite"
    );
}

#[test]
fn socket_problem_measures_the_resolved_path() {
    assert_eq!(SUN_PATH_MAX, 104);
    let (_dir, root) = canonical_temp();
    let (real, spelling) = symlinked_worktree(&root);
    let resolved_len = real.join(SOCKET_FILE).as_os_str().len();
    assert!(
        resolved_len < SUN_PATH_MAX,
        "environment precondition: temp dir too long ({resolved_len} bytes resolved)"
    );
    assert_eq!(
        socket_path_problem(&spelling),
        None,
        "a symlinked spelling whose resolved socket path fits must not be refused"
    );

    // A real directory chain whose own socket path is exactly 120 bytes.
    let suffix = format!("/{SOCKET_FILE}").len();
    let base_len = root.as_os_str().len();
    let pad = 120 - suffix - base_len - 1;
    let long_dir = root.join("b".repeat(pad));
    fs::create_dir_all(&long_dir).expect("create long dir");
    let long_socket = long_dir.join(SOCKET_FILE);
    assert_eq!(long_socket.as_os_str().len(), 120);

    let message = socket_path_problem(&long_dir).expect("a 120-byte socket path is refused");
    assert!(
        message.contains("120"),
        "message names the byte count: {message}"
    );
    assert!(
        message.contains("104"),
        "message names the limit: {message}"
    );
    assert!(
        message.contains(&long_socket.display().to_string()),
        "message names the path: {message}"
    );
}

#[test]
fn ready_then_abort_is_a_launch_failure() {
    let (_dir, root) = canonical_temp();
    let log = root.join("orchestrator.log");
    let (reader, writer) = std::io::pipe().expect("create pipe");
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg("ulimit -c 0; printf '\\002'; echo boom >>\"$LOG\"; printf '\\001'; kill -ABRT $$")
        .env("LOG", &log)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().expect("clone pipe writer"))
        .stderr(writer);
    let mut child = command.spawn().expect("spawn sh");
    drop(command);

    let timing = ReadyTiming {
        deadline: Duration::from_secs(5),
        grace: Duration::from_millis(500),
    };
    let result = await_ready(&mut child, reader, &log, timing);

    let error = match result {
        Ok(()) => panic!("a child that aborts after the ready byte must not be a ready daemon"),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.contains("SIGABRT"), "error names the signal: {error}");
    assert!(
        error.contains("boom"),
        "error carries the log tail: {error}"
    );
    let _ = child.try_wait();
}

#[test]
fn boot_id_prefers_the_session_variable() {
    assert_eq!(BOOT_ID_ENV, "LOOM_BOOT_ID");
    let value = "3F2B1C4D-0A1B-4C2D-8E3F-123456789ABC";

    let resolved = resolve_boot_id(Some(value), || -> anyhow::Result<String> {
        panic!("the OS boot-ID source must not be consulted when LOOM_BOOT_ID is UUID-shaped")
    })
    .expect("a UUID-shaped LOOM_BOOT_ID resolves");

    assert_eq!(resolved, value);
}

/// Writes `bin/wc`: runs the real `wc` and re-emits every count right-aligned in
/// 8 columns, as BSD `wc` does.
fn padding_wc(root: &Path) -> PathBuf {
    let found = Command::new("bash")
        .args(["-c", "command -v wc"])
        .output()
        .expect("locate wc");
    let real_wc = String::from_utf8(found.stdout).expect("utf-8 wc path");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("create bin");
    let shim = bin.join("wc");
    let script = format!(
        "#!/bin/sh\n\"{}\" \"$@\" | awk '{{ line = \"\"; for (i = 1; i <= NF; i++) {{ \
         if ($i ~ /^[0-9]+$/) line = line sprintf(\"%8d\", $i); else line = line \" \" $i }} \
         print line }}'\n",
        real_wc.trim()
    );
    fs::write(&shim, script).expect("write wc shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("chmod wc shim");
    bin
}

#[test]
fn padded_wc_resolves_the_start_row() {
    let (_dir, root) = canonical_temp();
    let bin = padding_wc(&root);
    let work = root.join("work");
    let ledger_dir = work.join("subagents").join("stage-a");
    fs::create_dir_all(&ledger_dir).expect("create ledger dir");
    let ledger = ledger_dir.join("starts.jsonl");
    let row = r#"{"agent_id":"reviewer-1","agent_type":"loom-code-reviewer","stage_id":"stage-a","parent_session_id":"parent-1","loom_session_id":"session-1","ts":"2000-01-01T00:00:00.000Z"}"#;
    fs::write(&ledger, format!("{row}\n")).expect("write starts.jsonl");

    let lifecycle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/_lifecycle.sh");
    let script = r#"padded=$(wc -c <"$3")
[[ "$padded" == " "* ]] || { echo "wc shim not active: '$padded'" >&2; exit 90; }
source "$1"
loom_lifecycle_resolve_start "$2" stage-a parent-1 session-1 reviewer-1 \
	loom-code-reviewer 2026-01-01T00:00:00.000Z contract-test
"#;
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .args(["-c", script, "contract"])
        .arg(&lifecycle)
        .arg(&work)
        .arg(&ledger)
        .env("PATH", path)
        .output()
        .expect("run bash");

    assert_eq!(
        output.status.code(),
        Some(0),
        "loom_lifecycle_resolve_start must accept a padded wc count; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
