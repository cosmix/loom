use super::*;

const UNSUPPORTED_400: &str = r#"ERROR: {"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'gpt-9-nonexistent' model is not supported when using Codex with a ChatGPT account."}}"#;

/// Whether `log` shows `model` unavailable — the runner's fallback test.
fn model_unavailable(log: &str, model: &str) -> bool {
    unavailable_line(log, model).is_some()
}

#[test]
fn model_unavailable_matches_the_captured_400_line() {
    let log = format!("starting codex\n{UNSUPPORTED_400}\n");
    assert_eq!(
        unavailable_line(&log, "gpt-9-nonexistent"),
        Some(UNSUPPORTED_400)
    );
}

#[test]
fn model_unavailable_matches_the_404_form() {
    assert!(model_unavailable(
        "ERROR: unexpected status 404 Not Found: Model not found gpt-6.1-sol",
        "gpt-6.1-sol"
    ));
}

#[test]
fn model_unavailable_ignores_quoted_text() {
    let quoted = "    \"404 Not Found: Model not found gpt-6.1-sol\",\n\
                  gpt-6.1-sol does not exist on some accounts";
    assert!(!model_unavailable(quoted, "gpt-6.1-sol"));
}

#[test]
fn model_unavailable_ignores_the_metadata_warning() {
    assert!(!model_unavailable(
        "warning: Model metadata for `gpt-6.1-sol` not found",
        "gpt-6.1-sol"
    ));
}

#[test]
fn model_unavailable_requires_the_model_on_the_same_line() {
    assert!(!model_unavailable(UNSUPPORTED_400, "gpt-6.1-sol"));
    let split = "running gpt-6.1-sol\nerror: the model is not supported";
    assert!(!model_unavailable(split, "gpt-6.1-sol"));
}

#[test]
fn model_unavailable_is_false_for_an_empty_log() {
    assert!(!model_unavailable("", "gpt-6.1-sol"));
}

#[test]
fn model_unavailable_line_is_what_the_fallback_note_carries() {
    let line = unavailable_line(UNSUPPORTED_400, "gpt-9-nonexistent").unwrap();
    let note = fallback_note("gpt-9-nonexistent", "gpt-9-other", line);
    assert!(note.starts_with(
        "codex model gpt-9-nonexistent is unavailable to this account — fell back to gpt-9-other\n    "
    ));
    assert!(note.ends_with("with a ChatGPT account.\"}}"));
}

#[test]
fn codex_model_label_shows_the_chain_only_when_there_is_one() {
    assert_eq!(
        codex_model_label("gpt-6.1-sol"),
        "gpt-6.1-sol (fallback: gpt-6-sol)"
    );
    assert_eq!(codex_model_label("gpt-6-astra"), "gpt-6-astra");
}

#[cfg(unix)]
#[test]
fn spawning_each_attempt_updates_the_shared_pid() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("exec"), "exit 0\n").unwrap();
    let pid = Arc::new(AtomicU32::new(0));
    let job = CodexJob {
        codex_path: PathBuf::from("/bin/sh"),
        repo_root: repo.path().to_path_buf(),
        skill: "$pressure plan.md".to_string(),
        log_path: repo.path().join("codex.log"),
        effort: "high".to_string(),
        pid: Arc::clone(&pid),
        control: Arc::new(Mutex::new(RunControl::default())),
    };
    let mut first = job.spawn("first").unwrap();
    let first_pid = first.id();
    let observed_first = pid.load(Ordering::Acquire);
    let mut second = job.spawn("second").unwrap();
    let second_pid = second.id();
    let observed_second = pid.load(Ordering::Acquire);
    first.wait().unwrap();
    second.wait().unwrap();
    assert_eq!(observed_first, first_pid);
    assert_eq!(observed_second, second_pid);
    assert_ne!(first_pid, second_pid);
}

#[cfg(unix)]
#[test]
fn join_returns_the_fallback_outcome_and_clears_the_pid() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("exec"),
        "for arg in \"$@\"; do\n\
         if [ \"$arg\" = missing ]; then\n\
         echo 'ERROR: model not found: missing'; exit 1\n\
         fi\n\
         done\nexit 0\n",
    )
    .unwrap();
    let run = spawn_codex_with_fallback(
        Path::new("/bin/sh"),
        repo.path(),
        "$pressure plan.md",
        &repo.path().join("codex.log"),
        vec!["missing".to_string(), "available".to_string()],
        "high",
    )
    .unwrap();
    let pid = Arc::clone(&run.pid);
    let outcome = run.join().unwrap();
    assert!(outcome.status.success());
    assert_eq!(outcome.model_used, "available");
    assert_eq!(outcome.tried, ["missing", "available"]);
    assert_eq!(outcome.notes.len(), 1);
    assert!(!outcome.exhausted);
    assert_eq!(pid.load(Ordering::Acquire), 0);
}

#[cfg(unix)]
#[test]
fn dropping_a_run_stops_and_reaps_its_active_attempt() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("exec"), "echo $$ > started\nsleep 30\n").unwrap();
    let run = spawn_codex_with_fallback(
        Path::new("/bin/sh"),
        repo.path(),
        "$pressure plan.md",
        &repo.path().join("codex.log"),
        vec!["first".into(), "fallback".into()],
        "high",
    )
    .unwrap();
    let pid = run.current_pid();
    let started = repo.path().join("started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !started.exists() {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(std::time::Duration::from_millis(10));
    }
    drop(run);
    assert!(matches!(
        nix::sys::signal::kill(Pid::from_raw(pid as i32), None),
        Err(Errno::ESRCH)
    ));
}

#[test]
fn polling_a_running_thread_and_joining_preserve_its_error() {
    let (release, wait) = std::sync::mpsc::channel();
    let run = CodexRun {
        handle: Some(thread::spawn(move || {
            wait.recv().unwrap();
            Err(anyhow!("attempt failed"))
        })),
        pid: Arc::new(AtomicU32::new(123)),
        control: Arc::new(Mutex::new(RunControl::default())),
    };
    assert_eq!(run.current_pid(), 123);
    assert!(!run.is_finished());
    release.send(()).unwrap();
    assert_eq!(run.join().err().unwrap().to_string(), "attempt failed");
}

#[test]
fn join_turns_a_runner_panic_into_an_error() {
    let run = CodexRun {
        handle: Some(thread::spawn(|| panic!("runner failed"))),
        pid: Arc::new(AtomicU32::new(0)),
        control: Arc::new(Mutex::new(RunControl::default())),
    };
    assert_eq!(
        run.join().err().unwrap().to_string(),
        "codex runner thread panicked"
    );
}
