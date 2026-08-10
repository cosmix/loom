//! Unit tests for the process helpers in the parent module.

use super::*;

#[test]
fn test_current_process_is_alive() {
    // Our own process should be alive
    let our_pid = std::process::id();
    assert!(is_process_alive(our_pid));
}

#[test]
fn test_nonexistent_process_is_not_alive() {
    // A very high PID is unlikely to exist
    assert!(!is_process_alive(999999999));
}

#[test]
fn unreaped_dead_child_is_not_alive() {
    // A child that has exited but not been waited on keeps its PID entry,
    // so `kill(pid, 0)` still succeeds for it. Liveness must answer no:
    // this is the state a tmux pane process lands in under
    // `remain-on-exit`, and treating it as alive makes a dead session look
    // like a working one forever.
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawning a trivial child should succeed");
    let pid = child.id();

    // Wait for the exit WITHOUT reaping: poll until the kernel reports the
    // zombie state, so the assertion cannot race the child's exit.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !process_is_zombie(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        process_is_zombie(pid),
        "child {pid} should be an unreaped zombie before the real assertion"
    );
    assert!(
        !is_process_alive(pid),
        "an unreaped zombie must not read as alive"
    );

    child.wait().expect("reaping the child should succeed");
    assert!(!is_process_alive(pid));
}

#[test]
fn test_pid_one_behavior() {
    // PID 1 is init/systemd, we may or may not be able to signal it
    // depending on permissions, so we just test it doesn't panic
    let _ = is_process_alive(1);
}

#[test]
fn test_pid_zero_kernel_process() {
    // PID 0 is the kernel scheduler process. We don't have permission to
    // signal it, so kill returns EPERM. The function should return true
    // because the process exists (EPERM means "exists but no permission").
    let result = is_process_alive(0);
    // On macOS and Linux, PID 0 exists (kernel) but we get EPERM.
    // With our EPERM handling, this should return true.
    assert!(
        result,
        "PID 0 (kernel) should be detected as alive via EPERM"
    );
}

#[test]
fn test_run_bounded_returns_output_when_command_completes() {
    let mut cmd = Command::new("echo");
    cmd.arg("loom");

    let output = run_bounded(&mut cmd, Duration::from_secs(10))
        .expect("echo should run")
        .completed()
        .expect("echo should complete well inside the deadline");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "loom");
}

#[test]
fn test_run_bounded_kills_command_that_outlives_deadline() {
    let mut cmd = Command::new("sleep");
    cmd.arg("60");

    let started = std::time::Instant::now();
    let outcome = run_bounded(&mut cmd, Duration::from_millis(200)).expect("sleep should spawn");

    assert!(
        matches!(outcome, BoundedOutput::TimedOut),
        "a 60s sleep must not report completion under a 200ms bound"
    );
    // The whole point is that the caller regains control promptly.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "run_bounded returned after {:?}; it must not wait for the child",
        started.elapsed()
    );
}

#[test]
fn bounded_output_returns_structured_timeout() {
    let mut cmd = Command::new("sleep");
    cmd.arg("60");

    let error = run_bounded_output(&mut cmd, Duration::from_millis(100), "git status")
        .expect_err("sleep must exceed the deadline");
    let timeout = error
        .downcast_ref::<ProcessTimeoutError>()
        .expect("timeout must be machine-identifiable");
    assert_eq!(timeout.operation(), "git status");
    assert_eq!(timeout.timeout(), Duration::from_millis(100));
}

#[cfg(unix)]
#[test]
fn timeout_kills_descendants_in_the_child_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let pid_path = temp.path().join("descendant.pid");
    let pid_arg = shell_escape::escape(pid_path.display().to_string().into());
    let script = format!("sleep 60 & printf '%s' $! > {pid_arg}; wait");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &script]);

    let outcome = run_bounded(&mut cmd, Duration::from_millis(200)).unwrap();
    assert!(matches!(outcome, BoundedOutput::TimedOut));
    let descendant_pid: u32 = std::fs::read_to_string(pid_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    for _ in 0..40 {
        if !is_process_alive(descendant_pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed-out command left descendant PID {descendant_pid} alive");
}

#[test]
fn test_terminate_reports_missing_process_without_error() {
    // ESRCH is success-with-nothing-to-do, not a failure.
    assert!(!terminate(999999999).expect("ESRCH must not error"));
}

#[test]
fn test_terminate_signals_live_process() {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sleep should spawn");

    assert!(terminate(child.id()).expect("signal should be delivered"));

    let status = child.wait().expect("child should be reapable");
    assert!(!status.success(), "SIGTERM'd sleep should not exit cleanly");
}

#[test]
fn test_u32_max_overflow_returns_false() {
    // u32::MAX exceeds i32::MAX, so the conversion fails.
    // The function should return false without panicking.
    assert!(
        !is_process_alive(u32::MAX),
        "u32::MAX should return false due to i32 overflow"
    );
}
