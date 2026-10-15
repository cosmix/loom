use super::*;
use std::sync::Arc;
use tempfile::tempdir;

/// Deadline for the tests that expect a reply. It only caps a broken
/// exchange: a working one returns the moment the reply line arrives, while
/// a loaded runner can hold the fake child for seconds before it writes, so
/// nothing is gained by keeping this near production's 15 s.
const REPLY_DEADLINE: Duration = Duration::from_secs(60);

#[test]
fn parse_snapshot_classifies_windows_by_duration() {
    let result = serde_json::json!({
        "rateLimits": {
            "primary": {"usedPercent": 42, "windowDurationMins": 300, "resetsAt": 1788531180},
            "secondary": {"usedPercent": 63.5, "windowDurationMins": 10080, "resetsAt": 1788728400000_i64},
            "planType": "pro"
        }
    });
    let quota = parse_snapshot(&result, 1_788_523_200);
    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 42.0);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[1].used_percent, 63.5);
    assert_eq!(quota.windows[1].resets_at, Some(1_788_728_400));
    assert_eq!(quota.plan, Some("pro".to_string()));
}

#[test]
fn parse_snapshot_ignores_an_unrecognized_window_duration() {
    let result = serde_json::json!({
        "rateLimits": { "primary": {"usedPercent": 10.0, "windowDurationMins": 60, "resetsAt": null} }
    });
    let quota = parse_snapshot(&result, 0);
    assert!(quota.windows.is_empty());
}

#[test]
fn parse_snapshot_with_no_rate_limits_yields_zero_windows_and_no_plan() {
    let quota = parse_snapshot(&serde_json::json!({}), 0);
    assert!(quota.windows.is_empty());
    assert_eq!(quota.plan, None);
}

#[cfg(unix)]
fn write_fake_codex(dir: &std::path::Path, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("codex");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Polls until the fake child has written its whole pid line and returns
/// the instant it saw it: the child's real start. A loaded runner can take
/// many seconds to get from `spawn` to the script's first line, so bounds
/// timed from this instant exclude that latency.
#[cfg(unix)]
fn wait_for_pid_file(pid_file: &Path) -> Option<Instant> {
    let give_up = Instant::now() + REPLY_DEADLINE;
    while Instant::now() < give_up {
        if std::fs::read_to_string(pid_file).is_ok_and(|pid| pid.ends_with('\n')) {
            return Some(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Runs `poll_once` with `REPLY_DEADLINE` beside a watcher thread waiting
/// for the fake child's pid file, and returns the result with the time from
/// the child's start to `poll_once` returning. With `stop_once_started`, the
/// watcher sets the shutdown flag as soon as the pid is on disk.
#[cfg(unix)]
fn poll_timed_from_child_start(
    codex_bin: &Path,
    pid_file: &Path,
    stop_once_started: bool,
) -> (Result<ProviderQuota>, Duration) {
    let shutdown = AtomicBool::new(false);
    let (result, started, finished) = std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            let started = wait_for_pid_file(pid_file);
            if stop_once_started {
                shutdown.store(true, Ordering::SeqCst);
            }
            started
        });
        let result = poll_once(codex_bin, REPLY_DEADLINE, &shutdown, 0);
        let finished = Instant::now();
        (result, watcher.join().unwrap(), finished)
    });
    let started = started.expect("the fake child never wrote its pid file");
    (result, finished.saturating_duration_since(started))
}

#[test]
#[cfg(unix)]
fn a_successful_exchange_parses_both_windows_and_the_plan() {
    let dir = tempdir().unwrap();
    let script = "#!/bin/sh\n\
        printf '%s\\n' '{\"id\":0,\"result\":{}}'\n\
        printf '%s\\n' '{\"method\":\"foo\",\"params\":{}}'\n\
        printf '%s\\n' '{\"id\":1,\"result\":{\"rateLimits\":{\"primary\":{\"usedPercent\":42,\"windowDurationMins\":300,\"resetsAt\":1788531180},\"secondary\":{\"usedPercent\":63.5,\"windowDurationMins\":10080,\"resetsAt\":1788728400000},\"planType\":\"pro\"}}}'\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = AtomicBool::new(false);

    let quota = poll_once(&codex_bin, REPLY_DEADLINE, &shutdown, 1_788_523_200).unwrap();

    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 42.0);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[1].used_percent, 63.5);
    assert_eq!(quota.windows[1].resets_at, Some(1_788_728_400));
    assert_eq!(quota.plan, Some("pro".to_string()));
}

#[test]
#[cfg(unix)]
fn a_json_rpc_error_reply_surfaces_the_message() {
    let dir = tempdir().unwrap();
    let script = "#!/bin/sh\n\
        printf '%s\\n' '{\"id\":1,\"error\":{\"code\":-32000,\"message\":\"not logged in\"}}'\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = AtomicBool::new(false);

    let error = poll_once(&codex_bin, REPLY_DEADLINE, &shutdown, 0).unwrap_err();
    assert!(error.to_string().contains("not logged in"));
}

#[test]
#[cfg(unix)]
fn garbage_and_an_over_long_line_are_skipped_before_the_reply() {
    let dir = tempdir().unwrap();
    let script = "#!/bin/sh\n\
        printf '%s\\n' 'hello'\n\
        printf '%70000s\\n' ''\n\
        printf '%s\\n' '{\"id\":1,\"result\":{\"rateLimits\":{}}}'\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = AtomicBool::new(false);

    let quota = poll_once(&codex_bin, REPLY_DEADLINE, &shutdown, 0).unwrap();
    assert!(quota.windows.is_empty());
}

#[test]
#[cfg(unix)]
fn a_hanging_server_times_out_and_the_child_is_killed() {
    let dir = tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let script = format!("#!/bin/sh\necho $$ > {}\nsleep 30\n", pid_file.display());
    let codex_bin = write_fake_codex(dir.path(), &script);

    // A fixed deadline races the child's start-up: under load the kill can
    // come before `echo $$` runs, leaving no pid to check. The watcher sets
    // `shutdown` only once the pid is on disk, which ends the reply wait
    // with the same "timed out" error; `a_passed_reply_deadline_times_out`
    // covers the deadline branch itself.
    let (result, elapsed) = poll_timed_from_child_start(&codex_bin, &pid_file, true);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "codex app-server timed out"
    );
    // `sleep 30` never reads stdin, so dropping it in teardown cannot make
    // the child exit early: the 2 s graceful `wait_timeout` elapses before
    // the process-group kill. `elapsed` runs from the child's start, so the
    // bound only has to rule out waiting for `sleep 30` itself.
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");

    let pid: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(!crate::process::is_process_alive(pid));
}

#[test]
#[cfg(unix)]
fn a_passed_reply_deadline_times_out() {
    let dir = tempdir().unwrap();
    // The deadline runs whether or not the child ever gets to run `sleep`,
    // so nothing here waits on the child's start-up.
    let codex_bin = write_fake_codex(dir.path(), "#!/bin/sh\nsleep 30\n");
    let shutdown = AtomicBool::new(false);

    let start = Instant::now();
    let result = poll_once(&codex_bin, Duration::from_secs(1), &shutdown, 0);
    let elapsed = start.elapsed();

    assert_eq!(
        result.unwrap_err().to_string(),
        "codex app-server timed out"
    );
    // Expected: spawn, the 1 s deadline, then the 2 s graceful wait and the
    // kill. Waiting on `sleep 30` instead would take at least 30 s, and the
    // bound leaves the rest for a loaded runner's slow spawn.
    assert!(elapsed < Duration::from_secs(25), "took {elapsed:?}");
}

#[test]
#[cfg(unix)]
fn a_flood_of_notifications_after_the_reply_does_not_hang_the_reader_join() {
    let dir = tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    // The reply to id 1 arrives first, so `await_reply` returns and stops
    // draining the channel; the 40 lines of noise that follow then overrun
    // its capacity-16 buffer. Without dropping the receiver before joining
    // the reader thread, the reader would block forever inside `send` and
    // `poll_once` would never return.
    let noise = "printf '%s\\n' '{\"id\":1,\"result\":{\"rateLimits\":{}}}'\n\
        for i in $(seq 40); do echo '{\"method\":\"noise\",\"params\":{}}'; done\n";
    let script = format!(
        "#!/bin/sh\necho $$ > {pid}\n{noise}sleep 30\n",
        pid = pid_file.display(),
    );
    let codex_bin = write_fake_codex(dir.path(), &script);

    let (result, elapsed) = poll_timed_from_child_start(&codex_bin, &pid_file, false);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // Without the receiver drop the join never returns; `elapsed` runs from
    // the child's start, so the bound only has to rule out that hang.
    assert!(elapsed < Duration::from_secs(20), "took {elapsed:?}");

    let pid: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(!crate::process::is_process_alive(pid));
}

#[test]
#[cfg(unix)]
fn the_child_exiting_without_ever_replying_is_reported_precisely() {
    let dir = tempdir().unwrap();
    // This script exits without ever reading stdin, so the parent's writes
    // may or may not win the race against its exit - the assertion holds
    // either way, which is the point of the fix.
    let script = "#!/bin/sh\nprintf '%s\\n' '{\"id\":0,\"result\":{}}'\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = AtomicBool::new(false);

    let error = poll_once(&codex_bin, REPLY_DEADLINE, &shutdown, 0).unwrap_err();

    assert_eq!(
        error.to_string(),
        "codex app-server closed without replying"
    );
}

#[test]
#[cfg(unix)]
fn a_child_that_closes_stdin_before_the_request_still_yields_its_reply() {
    let dir = tempdir().unwrap();
    // `exec 0<&-` drops the read end of the request pipe, so the writes in
    // `poll_once` hit a broken pipe whenever they lose the race to it. The
    // reply is still on stdout, and a broken request pipe must not discard it.
    let script = "#!/bin/sh\nexec 0<&-\n\
        printf '%s\\n' '{\"id\":1,\"result\":{\"rateLimits\":{\"primary\":{\"usedPercent\":10,\"windowDurationMins\":300,\"resetsAt\":null}}}}'\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = AtomicBool::new(false);

    let quota = poll_once(&codex_bin, REPLY_DEADLINE, &shutdown, 0).unwrap();

    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].used_percent, 10.0);
}

#[test]
#[cfg(unix)]
fn a_shutdown_flag_set_mid_exchange_returns_quickly() {
    let dir = tempdir().unwrap();
    let script = "#!/bin/sh\nsleep 30\n";
    let codex_bin = write_fake_codex(dir.path(), script);
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, Ordering::SeqCst);
    });

    let start = std::time::Instant::now();
    let result = poll_once(&codex_bin, Duration::from_secs(30), &shutdown, 0);
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // The reply-wait loop notices the flag within one `RECV_SLICE`, and
    // `sleep 30` ignores the stdin close in teardown, so the full 2 s
    // graceful `wait_timeout` still elapses before the kill. The bound
    // only has to rule out waiting for the 30 s deadline.
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");
}
