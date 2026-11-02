//! Stopping a pressure step, its Codex run and its report.

use super::*;
#[cfg(target_os = "linux")]
use crate::pressure::codex_run::spawn_codex_with_fallback;
#[cfg(target_os = "linux")]
use crate::pressure::PressureDriver;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "linux")]
use std::time::Instant;

/// `exec` for a fake Codex attempt run through `/bin/sh` in the repo: a
/// descendant in its group that records its pid and outlives the leader's
/// `wait` unless the whole group is stopped.
#[cfg(target_os = "linux")]
const WAITING_CODEX: &str =
    "sh -c 'echo $$ > descendant.pid; sleep 30; echo review > review.md' &\nwait\n";

#[cfg(target_os = "linux")]
fn descendant_of(repo: &Path) -> u32 {
    wait_for_file(&repo.join("descendant.pid"));
    // The shell creates the file before it writes the pid.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(pid) = fs::read_to_string(repo.join("descendant.pid"))
            .unwrap()
            .trim()
            .parse()
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "descendant.pid stayed empty");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Ctrl-C at Claude's prompt is a keystroke, not a signal, so a Claude that
/// exits 130 is the only sign of it. The run stops either way and deletes
/// the report, so Codex has nothing left to contribute.
#[cfg(target_os = "linux")]
#[test]
fn an_interrupted_claude_retires_codex_instead_of_waiting_for_it() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    fs::write(root.join("exec"), WAITING_CODEX).unwrap();
    let claude = root.join("claude");
    fs::write(
        &claude,
        "#!/bin/sh\nwhile [ ! -s descendant.pid ]; do sleep 0.01; done\nexit 130\n",
    )
    .unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let models = crate::pressure::models::PressureModels {
        claude: "model".into(),
        claude_effort: "high".into(),
        codex: "codex-model".into(),
        codex_effort: "high".into(),
        address: "model".into(),
        address_effort: "high".into(),
    };
    let mut driver = TerminalDriver {
        claude_path: &claude,
        codex_path: Path::new("/bin/sh"),
        repo_root: root,
        marker: &root.join("marker"),
        codex_log: &root.join("codex.log"),
        report: &root.join("review.md"),
        models: &models,
        retirement: CodexRetirement::default(),
    };

    let started = Instant::now();
    let result = driver
        .run_pressure(1, "/pressure plan.md", "$pressure plan.md")
        .unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the driver waited for a run the user interrupted"
    );
    let StepVerdict::Stop { step, reason } = result.verdict else {
        panic!("an interrupted Claude stops the run");
    };
    assert_eq!(step, "round 1 /pressure");
    assert!(reason.contains("claude exited with code 130"), "{reason}");
    wait_for_exit(descendant_of(root));
}

/// The handler in a child process: one is allowed per process, and it ends
/// the process it runs in.
#[cfg(target_os = "linux")]
#[test]
fn a_termination_signal_retires_codex_and_exits_130() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    const REPO_ENV: &str = "LOOM_TEST_PRESSURE_SIGNAL_REPO";
    if let Some(repo) = std::env::var_os(REPO_ENV) {
        let repo = Path::new(&repo);
        let retirement = CodexRetirement::default();
        retire_on_signal(retirement.clone()).unwrap();
        let _run = retirement
            .spawn_with_fallback(
                Path::new("/bin/sh"),
                repo,
                "$pressure plan.md",
                &repo.join("codex.log"),
                vec!["first".into()],
                "high",
            )
            .unwrap();
        thread::sleep(Duration::from_secs(60));
        panic!("no signal arrived");
    }
    for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join("exec"), WAITING_CODEX).unwrap();
        let mut driver = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::pressure::terminal::tests::\
                 a_termination_signal_retires_codex_and_exits_130",
                "--nocapture",
            ])
            .env(REPO_ENV, repo.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let descendant = descendant_of(repo.path());

        kill(Pid::from_raw(driver.id() as i32), signal).unwrap();

        assert_eq!(driver.wait().unwrap().code(), Some(130), "{signal}");
        wait_for_exit(descendant);
    }
}

#[test]
fn stop_verdict_identifies_the_round_and_step_and_preserves_the_reason() {
    let reason = "claude exited with code 130 — stopping pressure run.";
    for slash in ["/pressure", "/address"] {
        assert_eq!(
            step_verdict(2, slash, Some(reason.to_string())),
            StepVerdict::Stop {
                step: format!("round 2 {slash}"),
                reason: reason.to_string(),
            }
        );
    }
    assert_eq!(step_verdict(2, "/address", None), StepVerdict::Continue);
}

#[cfg(target_os = "linux")]
#[test]
fn foreground_error_stops_codex_descendants_before_report_cleanup() {
    let repo = tempfile::tempdir().unwrap();
    let run = active_fake_codex(repo.path());
    let leader = run.current_pid();
    let error = run_foreground_with_codex(run, || {
        wait_for_file(&repo.path().join("descendant.pid"));
        run_claude_foreground(
            &repo.path().join("missing-claude"),
            repo.path(),
            "/pressure plan.md",
            &repo.path().join("marker"),
            "test",
            "high",
        )
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "failed to spawn claude");
    let descendant: u32 = fs::read_to_string(repo.path().join("descendant.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    wait_for_exit(leader);
    wait_for_exit(descendant);
    assert_eq!(
        fs::read_to_string(repo.path().join("attempts")).unwrap(),
        "first\n"
    );
    crate::claude::remove_if_exists(&repo.path().join("review.md")).unwrap();
    fs::write(repo.path().join("release"), "go").unwrap();
    assert!(!repo.path().join("review.md").exists());
}

#[cfg(target_os = "linux")]
fn active_fake_codex(repo: &Path) -> CodexRun {
    let script = "for arg in \"$@\"; do\n\
                  if [ \"$prev\" = -m ]; then model=$arg; fi\n\
                  prev=$arg\n\
                  done\n\
                  echo \"$model\" >> attempts\n\
                  sh -c 'echo $$ > descendant.pid; while [ ! -e release ]; do sleep 0.05; done; echo review > review.md' &\n\
                  echo $! > child.pid\n\
                  echo \"ERROR: model not found: $model\" >&2\n\
                  wait\n";
    fs::write(repo.join("exec"), script).unwrap();
    spawn_codex_with_fallback(
        Path::new("/bin/sh"),
        repo,
        "$pressure plan.md",
        &repo.join("codex.log"),
        vec!["first".into(), "fallback".into()],
        "high",
    )
    .unwrap()
}

#[cfg(target_os = "linux")]
fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn process_running(pid: u32) -> bool {
    let path = format!("/proc/{pid}/stat");
    fs::read_to_string(path).ok().is_some_and(|stat| {
        stat.rsplit_once(") ")
            .is_some_and(|(_, tail)| !tail.starts_with('Z'))
    })
}

#[cfg(target_os = "linux")]
fn wait_for_exit(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while process_running(pid) {
        assert!(
            Instant::now() < deadline,
            "process {pid} outlived cancellation"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
