use crate::process::sandbox_probe::skip_without_group_signals;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::process::is_process_alive;
use crate::process::sandbox_probe::skip_without_pty;

use super::protocol::WindowSize;
use super::pty::{open_pty, PtyChild};
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

fn spawn_shell(script: &str) -> PtyChild {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    PtyChild::spawn(&mut command, WindowSize { cols: 80, rows: 24 }).expect("spawn shell in PTY")
}

fn write_bytes(child: &PtyChild, bytes: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut written = 0;
    while written < bytes.len() && Instant::now() < deadline {
        match child.write_some(&bytes[written..]).expect("write PTY") {
            0 => thread::sleep(Duration::from_millis(10)),
            count => written += count,
        }
    }
    assert_eq!(written, bytes.len(), "PTY did not accept test input");
}

fn read_until(child: &PtyChild, needle: &[u8], timeout: Duration) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut output = Vec::new();
    while Instant::now() < deadline {
        let mut bytes = [0u8; 4096];
        match child.read(&mut bytes).expect("read PTY") {
            Some(0) => break,
            Some(count) => output.extend_from_slice(&bytes[..count]),
            None => thread::sleep(Duration::from_millis(10)),
        }
        if output.windows(needle.len()).any(|part| part == needle) {
            break;
        }
    }
    output
}

#[test]
fn pty_child_echoes_input() {
    if skip_without_pty("pty_child_echoes_input") {
        return;
    }
    let child = spawn_shell("read line; printf 'got:%s\\n' \"$line\"");
    write_bytes(&child, b"hi\n");
    let output = read_until(&child, b"got:hi", Duration::from_secs(10));
    child.shutdown();
    assert!(
        output.windows(6).any(|part| part == b"got:hi"),
        "PTY output was {:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn pty_child_resize_is_visible_to_stty() {
    if skip_without_pty("pty_child_resize_is_visible_to_stty") {
        return;
    }
    let child = spawn_shell("read x; stty size");
    child
        .resize(WindowSize {
            cols: 120,
            rows: 40,
        })
        .expect("resize PTY");
    write_bytes(&child, b"\n");
    let output = read_until(&child, b"40 120", Duration::from_secs(10));
    child.shutdown();
    assert!(
        output.windows(6).any(|part| part == b"40 120"),
        "PTY output was {:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn pty_master_is_close_on_exec() {
    if skip_without_pty("pty_master_is_close_on_exec") {
        return;
    }
    let (master, _slave) = open_pty(WindowSize { cols: 80, rows: 24 }).expect("open PTY");
    let flags = fcntl(&master, FcntlArg::F_GETFD).expect("read descriptor flags");
    assert!(FdFlag::from_bits_truncate(flags).contains(FdFlag::FD_CLOEXEC));
}

#[test]
fn pty_slave_is_close_on_exec_and_peers_the_master() {
    if skip_without_pty("pty_slave_is_close_on_exec_and_peers_the_master") {
        return;
    }
    let (master, slave) = open_pty(WindowSize { cols: 80, rows: 24 }).expect("open PTY");
    // `openpty(3)` on other platforms hands out a slave without the flag.
    #[cfg(target_os = "linux")]
    {
        let flags = fcntl(&slave, FcntlArg::F_GETFD).expect("read descriptor flags");
        assert!(FdFlag::from_bits_truncate(flags).contains(FdFlag::FD_CLOEXEC));
    }
    let mut slave = File::from(slave);
    let mut master = File::from(master);
    slave.write_all(b"peer-check\n").expect("write to slave");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = Vec::new();
    while Instant::now() < deadline && !seen.windows(10).any(|part| part == b"peer-check") {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
        let mut fds = [PollFd::new(master.as_fd(), PollFlags::POLLIN)];
        if poll(&mut fds, timeout).expect("poll master") == 0 {
            break;
        }
        let mut bytes = [0u8; 256];
        let count = master.read(&mut bytes).expect("read master");
        seen.extend_from_slice(&bytes[..count]);
    }
    assert!(
        seen.windows(10).any(|part| part == b"peer-check"),
        "master saw {:?}",
        String::from_utf8_lossy(&seen)
    );
}

#[test]
fn pty_child_shutdown_reaps_the_child() {
    let false = skip_without_group_signals("pty_child_shutdown_reaps_the_child") else {
        return;
    };
    if skip_without_pty("pty_child_shutdown_reaps_the_child") {
        return;
    }
    let child = spawn_shell("sleep 30");
    let pid = child.pid();
    let status = child.shutdown().expect("shutdown reaps the child");
    // The hangup ends the shell with SIGHUP; had it not reached the child,
    // the kill fallback would show SIGKILL. How the child ended says which
    // path ran; a wall-clock bound cannot tell them apart without tying the
    // test to the machine's load.
    assert_eq!(
        status.signal(),
        Some(libc::SIGHUP),
        "child {pid} ended as {status:?}, not by the hangup"
    );
    assert!(!is_process_alive(pid), "child {pid} remains alive");
}

#[test]
fn pty_child_resize_delivers_sigwinch() {
    if skip_without_pty("pty_child_resize_delivers_sigwinch") {
        return;
    }
    let child = spawn_shell("trap 'stty size' WINCH; read x");
    thread::sleep(Duration::from_millis(100));
    child
        .resize(WindowSize {
            cols: 120,
            rows: 40,
        })
        .expect("resize PTY");
    let output = read_until(&child, b"40 120", Duration::from_secs(5));
    child.shutdown();
    assert!(
        output.windows(6).any(|part| part == b"40 120"),
        "PTY output was {:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn pty_write_survives_a_child_that_stops_reading() {
    let false = skip_without_group_signals("pty_write_survives_a_child_that_stops_reading") else {
        return;
    };
    if skip_without_pty("pty_write_survives_a_child_that_stops_reading") {
        return;
    }
    // Raw mode, as tmux sets it, and only once the child says it is set. A
    // canonical-mode PTY holding no complete line never refuses the master's
    // writes on Darwin: input past MAX_INPUT is discarded with a bell instead,
    // so an unterminated flood there would never see backpressure at all.
    let child = spawn_shell("stty raw -echo; printf ready; sleep 30");
    let ready = read_until(&child, b"ready", Duration::from_secs(10));
    assert!(
        ready.windows(5).any(|part| part == b"ready"),
        "PTY output was {:?}",
        String::from_utf8_lossy(&ready)
    );
    let payload = vec![b'x'; 1024 * 1024];
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saturated = false;
    while Instant::now() < deadline {
        match child.write_some(&payload) {
            Ok(0) => {
                saturated = true;
                break;
            }
            Ok(_) => {}
            Err(error) => panic!("write to non-reading PTY failed: {error}"),
        }
    }
    child.shutdown();
    assert!(saturated, "PTY write did not report backpressure");
}
