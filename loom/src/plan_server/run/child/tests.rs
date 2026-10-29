use super::*;

fn shell(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", script]);
    cmd
}

#[test]
fn drains_one_mib_from_each_stream_without_losing_lines() {
    // 16,384 lines of 63 bytes plus a newline = exactly 1 MiB per stream.
    let line = "x".repeat(63);
    let script = format!(
        "i=0; while [ \"$i\" -lt 16384 ]; do \
         printf '%s\\n' '{line}'; printf '%s\\n' '{line}' >&2; \
         i=$((i + 1)); done"
    );
    let mut stdout = 0;
    let mut stderr = 0;
    let status = run_streamed_bounded(shell(&script), Duration::from_secs(10), |stream, text| {
        assert_eq!(text, line);
        match stream {
            OutputStream::Stdout => stdout += 1,
            OutputStream::Stderr => stderr += 1,
        }
    })
    .unwrap();
    assert!(status.success());
    assert_eq!(stdout, 16_384);
    assert_eq!(stderr, 16_384);
}

#[test]
fn truncates_long_lines_at_a_utf8_boundary_and_keeps_following_lines() {
    let long = "€".repeat(2_000);
    let script = format!("printf '%s\\n' '{long}' '<b>&raw</b>'");
    let mut lines = Vec::new();
    let status = run_streamed(shell(&script), |stream, line| {
        assert_eq!(stream, OutputStream::Stdout);
        lines.push(line.to_owned());
    })
    .unwrap();
    assert!(status.success());
    assert_eq!(lines, vec!["€".repeat(1_365), "<b>&raw</b>".to_owned()]);
    assert_eq!(lines[0].len(), 4_095);
}

#[test]
fn kills_a_sleeping_child_at_the_bound() {
    let mut pid = None;
    let started = Instant::now();
    let error = run_streamed_bounded(
        shell("printf '%s\\n' \"$$\"; sleep 30 & wait"),
        Duration::from_millis(200),
        |_, line| pid = Some(line.parse::<i32>().unwrap()),
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    assert_eq!(error.to_string(), "child exceeded 30 minutes");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(
        nix::sys::signal::kill(Pid::from_raw(pid.expect("child printed its pid")), None),
        Err(nix::errno::Errno::ESRCH),
    );
}

#[test]
fn only_one_job_runs_until_its_guard_is_dropped() {
    let jobs = Arc::new(RunJobs::default());
    assert_eq!(jobs.current(), None);
    let guard = jobs.begin(RunJob::Start).unwrap();
    assert_eq!(jobs.current(), Some(RunJob::Start));
    assert!(jobs.begin(RunJob::Commit).is_none());
    thread::spawn(move || drop(guard)).join().unwrap();
    assert_eq!(jobs.current(), None);
    let next = jobs.begin(RunJob::Commit).unwrap();
    assert_eq!(jobs.current(), Some(RunJob::Commit));
    drop(next);
    assert_eq!(jobs.current(), None);
}
