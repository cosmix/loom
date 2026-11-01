//! A killed TUI leaves raw mode on its PTY; the next loom process must repair
//! that same PTY without touching a different one.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use std::fs::File;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;
use std::process::{Command, Stdio};

use nix::pty::openpty;
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, LocalFlags, SetArg};

const HELPER_ENV: &str = "LOOM_TERMINAL_RECOVERY_HELPER";

fn run_loom_on_pty(base: &Path, slave: &File) -> ExitStatus {
    helpers::loom_cmd()
        .arg("--version")
        .current_dir(base)
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::null())
        .status()
        .unwrap()
}

#[test]
fn killed_tui_helper() {
    let Ok(work_path) = std::env::var(HELPER_ENV) else {
        return;
    };
    loom::utils::write_tui_marker(std::path::Path::new(&work_path));
    let stdout = std::io::stdout();
    let mut attributes = tcgetattr(&stdout).unwrap();
    cfmakeraw(&mut attributes);
    tcsetattr(&stdout, SetArg::TCSANOW, &attributes).unwrap();
    unsafe { libc::raise(libc::SIGKILL) };
    unreachable!("SIGKILL must terminate the helper");
}

#[test]
fn fresh_process_recovers_only_the_marked_pty() {
    let scratch = tempfile::tempdir().unwrap();
    let base = scratch.path();
    let work = loom::fs::work_dir::WorkDir::new(base).unwrap();
    work.initialize().unwrap();
    let work_path = work.root();
    let marker = work_path.join("tui.pid");
    let pair = openpty(None, None).unwrap();
    let slave = File::from(pair.slave);

    let killed = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "killed_tui_helper", "--nocapture"])
        .env(HELPER_ENV, work_path)
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(killed.signal(), Some(libc::SIGKILL));
    assert!(marker.exists());
    let raw = tcgetattr(&slave).unwrap();
    assert!(!raw
        .local_flags
        .contains(LocalFlags::ICANON | LocalFlags::ECHO));

    let other = openpty(None, None).unwrap();
    let other_slave = File::from(other.slave);
    let wrong_terminal = run_loom_on_pty(base, &other_slave);
    assert!(wrong_terminal.success());
    assert!(
        marker.exists(),
        "a different PTY must leave the marker for retry"
    );
    assert!(!tcgetattr(&slave)
        .unwrap()
        .local_flags
        .contains(LocalFlags::ICANON));

    let recovered = run_loom_on_pty(base, &slave);
    assert!(recovered.success());
    let restored = tcgetattr(&slave).unwrap();
    assert!(restored
        .local_flags
        .contains(LocalFlags::ICANON | LocalFlags::ECHO));
    assert!(!marker.exists(), "successful recovery clears the marker");
}
