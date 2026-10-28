//! `tmux_io`: the pane line parser, the failure paths, and a real tmux round trip that skips
//! where tmux is missing or the sandbox refuses unix sockets.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::orchestrator::terminal::tmux::socket::tests::{
    server_socket_dir, TmuxServerGuard, TmuxTmpDirGuard,
};
use crate::orchestrator::terminal::tmux::{socket_path_for, spawn_in_tmux, teardown_socket};
use crate::process::sandbox_probe::{skip_unless, unix_socket_bindable};

/// A `TMUX_TMPDIR` pinned to a fresh directory where a real tmux server can start.
/// `_pin` is declared first so the variable is restored before the directory goes.
pub(crate) struct RealTmux {
    _pin: TmuxTmpDirGuard,
    pub(crate) dir: TempDir,
}

/// The pinned directory, or `None` after a `SKIP` line when tmux is missing or this sandbox
/// cannot bind a unix socket. Callers are `#[serial]`: the pin is process-global.
pub(crate) fn real_tmux(test: &str) -> Option<RealTmux> {
    let dir = server_socket_dir();
    let tmux = std::process::Command::new("tmux")
        .arg("-V")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success());
    let why = "needs a tmux binary and a sandbox that lets it bind its unix socket";
    if skip_unless(tmux && unix_socket_bindable(dir.path()), test, why) {
        return None;
    }
    Some(RealTmux {
        _pin: TmuxTmpDirGuard::set(dir.path()),
        dir,
    })
}

/// Poll `ready` every 50 ms for up to `limit`.
pub(crate) fn wait_until(limit: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ready()
}

#[test]
fn a_live_pane_reports_its_pid() {
    assert_eq!(parse_pane("4242:0:").unwrap(), 4242);
}

#[test]
fn a_dead_pane_is_an_error_naming_its_exit_status() {
    let error = parse_pane("4242:1:3").unwrap_err();
    assert!(
        format!("{error:#}").contains("already exited (status 3)"),
        "{error:#}"
    );
}

#[test]
fn a_line_without_a_pid_is_an_error() {
    for line in ["", "0:0:", "abc:0:", ":0:"] {
        assert!(parse_pane(line).is_err(), "{line:?} parsed as a pid");
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "unsanitised C0 control")]
fn an_unsanitised_control_character_is_refused_before_tmux_runs() {
    let _ = paste_text("loom-plan-unused", "plan-authoring", "a\x1b[2Jb");
}

#[test]
#[serial]
fn a_paste_to_a_missing_server_fails_without_blocking_the_feeder() {
    // More than a pipe buffer: a tmux client that exits without reading leaves the feeder
    // blocked forever unless the parent's read end is closed before the join.
    let dir = TempDir::new().unwrap();
    let _pin = TmuxTmpDirGuard::set(dir.path());
    let text = "x".repeat(200 * 1024);
    let error = paste_text("loom-plan-absent0000", "plan-authoring", &text).unwrap_err();
    assert!(format!("{error:#}").contains("load-buffer"), "{error:#}");
    let pressed = press("loom-plan-absent0000", "plan-authoring", "Escape");
    assert!(
        pressed.is_err(),
        "a key press reached no server yet succeeded"
    );
}

/// About 120 KB of 99-character lines: a terminal in canonical mode truncates a line past
/// 4095 bytes, so the round trip keeps lines short.
fn long_text() -> String {
    let body = "abcdefghij".repeat(10);
    let lines: Vec<String> = (0..1240)
        .map(|n| format!("{n:05} {}", &body[..93]))
        .collect();
    lines.join("\n")
}

/// Whether `pid` is `cat`, or the shell tmux started that runs `cat` as its child.
fn runs_cat(pid: u32) -> bool {
    if !cfg!(target_os = "linux") {
        return crate::process::process_start_time(pid).is_some();
    }
    let comm = |pid: &str| fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    if comm(&pid.to_string()).trim() == "cat" {
        return true;
    }
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stat = fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
        let parent = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.split(' ').nth(1));
        parent == Some(pid.to_string().as_str()) && comm(&name).trim() == "cat"
    })
}

/// A script tmux runs in the pane: `cat` into `out`.
fn cat_script(dir: &Path, out: &Path) -> std::path::PathBuf {
    let script = dir.join("cat.sh");
    let target = shell_escape::escape(out.to_string_lossy()).into_owned();
    fs::write(&script, format!("#!/bin/sh\nexec cat > {target}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    script
}

#[test]
#[serial]
fn a_long_paste_reaches_the_pane_and_teardown_removes_the_socket() {
    let test = "a_long_paste_reaches_the_pane_and_teardown_removes_the_socket";
    let Some(tmux) = real_tmux(test) else {
        return;
    };
    let socket = format!("loom-plan-io{}", std::process::id());
    let _server = TmuxServerGuard::new(&socket);
    let out = tmux.dir.path().join("pasted.txt");
    let script = cat_script(tmux.dir.path(), &out);
    spawn_in_tmux(&socket, "plan-io", tmux.dir.path(), &script).unwrap();

    let text = long_text();
    assert!(text.len() > 120_000);
    paste_text(&socket, "plan-io", &text).unwrap();
    // paste-buffer turns each newline into Enter and the tty turns it back; the final Enter
    // submits the last line.
    let expected = format!("{text}\n");
    let complete = || fs::metadata(&out).is_ok_and(|m| m.len() >= expected.len() as u64);
    assert!(
        wait_until(Duration::from_secs(20), complete),
        "paste incomplete"
    );
    assert_eq!(fs::read_to_string(&out).unwrap(), expected);

    let pid = pane_pid(&socket, "plan-io").unwrap();
    assert!(runs_cat(pid), "pane pid {pid} is not the cat process");

    teardown_socket(&socket).unwrap();
    assert!(!socket_path_for(&socket).exists(), "the socket survived");
}
