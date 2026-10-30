use std::os::unix::process::ExitStatusExt;
use std::time::Duration;

use super::*;

#[test]
fn summary_args_preserve_sandbox_paths_model_effort_and_prompt() {
    let root = Path::new("/repo with spaces");
    let runtime = Path::new("/runtime with spaces");
    let prompt = "summarize $value, 'quoted' and {braced}\nverbatim";

    let args = codex_summary_args("test-model", "high", root, runtime, prompt);

    assert_eq!(
        args,
        [
            "exec",
            "--sandbox",
            "workspace-write",
            "--add-dir",
            "/runtime with spaces",
            "-m",
            "test-model",
            "-c",
            "model_reasoning_effort=high",
            "-C",
            "/repo with spaces",
            prompt,
        ]
        .map(str::to_owned)
    );
    assert_eq!(args.last().map(String::as_str), Some(prompt));
    assert!(!args.iter().any(|arg| arg.contains("danger-full-access")));
}

#[test]
fn summary_args_add_runtime_directory_after_sandbox_selection() {
    let args = codex_summary_args(
        "model",
        "low",
        Path::new("/repo"),
        Path::new("/runtime"),
        "prompt",
    );

    let sandbox = args.iter().position(|arg| arg == "workspace-write");
    let add_dir = args.iter().position(|arg| arg == "--add-dir");

    assert!(
        matches!((sandbox, add_dir), (Some(sandbox), Some(add_dir)) if sandbox < add_dir),
        "--add-dir must follow workspace-write: {args:?}"
    );
}

#[test]
fn render_prompt_replaces_placeholders_and_lists_each_id_once() {
    let ids = ["summary-alpha".to_owned(), "summary-beta".to_owned()];

    let prompt = render_prompt(&ids, Path::new("/runtime directory/summaries"));

    assert!(!prompt.contains("{{"), "unexpanded placeholder: {prompt}");
    assert!(prompt.contains("/runtime directory/summaries"));
    assert!(prompt.contains("- summary-alpha\n- summary-beta"));
    for id in &ids {
        let bullet = format!("- {id}");
        assert_eq!(prompt.lines().filter(|line| *line == bullet).count(), 1);
        assert_eq!(prompt.matches(id.as_str()).count(), 1);
    }
}

#[test]
fn render_prompt_preserves_dollars_quotes_and_braces_in_ids() {
    let ids = [
        "dollar-$HOME".to_owned(),
        "quoted-\"double\"-'single'".to_owned(),
        "braced-{literal}".to_owned(),
    ];

    let prompt = render_prompt(&ids, Path::new("/runtime"));

    for id in &ids {
        let bullet = format!("- {id}");
        assert_eq!(prompt.lines().filter(|line| *line == bullet).count(), 1);
    }
}

#[test]
fn exit_outcome_success_closes_even_with_log_and_nonzero_code() {
    assert_eq!(
        exit_outcome(true, Some(2), "error text"),
        EndOutcome::Closed
    );
}

#[test]
fn exit_outcome_failure_uses_last_nonempty_log_line_and_trims_it() {
    assert_eq!(
        exit_outcome(false, Some(2), "first error\n\n  final error \t\n \t\n"),
        EndOutcome::Failed("final error".to_owned())
    );
}

#[test]
fn exit_outcome_empty_log_reports_exit_code() {
    assert_eq!(
        exit_outcome(false, Some(2), ""),
        EndOutcome::Failed("codex exited with status 2".to_owned())
    );
}

#[test]
fn exit_outcome_without_code_reports_signal() {
    assert_eq!(
        exit_outcome(false, None, " \n\t\n"),
        EndOutcome::Failed("codex was killed by a signal".to_owned())
    );
}

#[test]
fn exit_outcome_truncates_500_character_line_to_first_200_characters() {
    let log = format!("{}{}", "a".repeat(200), "z".repeat(300));

    assert_eq!(
        exit_outcome(false, Some(2), &log),
        EndOutcome::Failed("a".repeat(200))
    );
}

#[test]
fn exit_outcome_truncates_by_characters_for_multibyte_text() {
    let log = "é".repeat(500);

    assert_eq!(
        exit_outcome(false, None, &log),
        EndOutcome::Failed("é".repeat(200))
    );
}

#[test]
fn a_stopped_job_ends_as_a_teardown_whatever_its_exit() {
    for outcome in [
        EndOutcome::Closed,
        EndOutcome::Failed("codex was killed by a signal".to_owned()),
    ] {
        assert_eq!(final_outcome(true, outcome), EndOutcome::TornDown);
    }
}

#[test]
fn a_job_nobody_stopped_keeps_its_exit_outcome() {
    let failed = EndOutcome::Failed("boom".to_owned());
    assert_eq!(final_outcome(false, failed.clone()), failed);
    assert_eq!(final_outcome(false, EndOutcome::Closed), EndOutcome::Closed);
}

#[test]
fn only_codex_ids_are_codex_sessions() {
    assert!(is_codex_session("codex-1761000000000-0"));
    assert!(!is_codex_session("0f8fad5b-d9cb-469f-a165-70867728950e"));
}

#[test]
fn prune_logs_keeps_the_newest_files_only() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let total = KEPT_LOGS + 3;
    for index in 0..total {
        let path = dir.path().join(format!("codex-{index:03}.log"));
        let file = File::create(&path).expect("log");
        let age = Duration::from_secs((total - index) as u64 * 10);
        file.set_modified(SystemTime::now() - age).expect("mtime");
    }
    prune_logs(dir.path());
    let mut kept: Vec<String> = std::fs::read_dir(dir.path())
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    kept.sort();
    assert_eq!(kept.len(), KEPT_LOGS);
    assert_eq!(kept.first().map(String::as_str), Some("codex-003.log"));
}

fn job_for(root: &Path, pid: Pid, stopping: bool) -> Job {
    Job {
        pid,
        before: config_guard::snapshot(root).expect("config snapshot"),
        codex_before: codex_snapshot(root).expect("codex snapshot"),
        stopping,
    }
}

fn jobs_with(id: &str, job: Job) -> Jobs {
    let mut registry = Registry::default();
    registry.jobs.insert(id.to_owned(), job);
    Arc::new(Mutex::new(registry))
}

/// What the watcher sees for a job that a signal ended.
fn settle_signalled(stopping: bool) -> (EndOutcome, bool) {
    let root = tempfile::TempDir::new().expect("temp dir");
    let job = job_for(root.path(), Pid::from_raw(i32::MAX), stopping);
    let jobs = jobs_with("codex-1-0", job);
    let signalled = std::process::ExitStatus::from_raw(15);
    let outcome = settle_outcome(
        &jobs,
        root.path(),
        "codex-1-0",
        Ok(signalled),
        &root.path().join("missing.log"),
    );
    let left = lock(&jobs).jobs.contains_key("codex-1-0");
    (outcome, left)
}

#[test]
fn a_terminated_job_settles_as_a_teardown_and_leaves_the_registry() {
    assert_eq!(settle_signalled(true), (EndOutcome::TornDown, false));
}

#[test]
fn a_job_killed_without_a_stop_request_settles_as_failed() {
    assert_eq!(
        settle_signalled(false),
        (
            EndOutcome::Failed("codex was killed by a signal".to_owned()),
            false
        )
    );
}

#[test]
fn a_changed_configuration_outranks_a_requested_stop() {
    let root = tempfile::TempDir::new().expect("temp dir");
    let job = job_for(root.path(), Pid::from_raw(i32::MAX), true);
    let jobs = jobs_with("codex-1-0", job);
    let codex_dir = root.path().join(".codex");
    std::fs::create_dir(&codex_dir).expect("codex dir");
    std::fs::write(codex_dir.join("config.toml"), "model = \"x\"\n").expect("codex config");

    let outcome = settle_outcome(
        &jobs,
        root.path(),
        "codex-1-0",
        Ok(std::process::ExitStatus::from_raw(15)),
        &root.path().join("missing.log"),
    );

    assert_eq!(
        outcome,
        EndOutcome::Failed("codex changed the .claude or .codex configuration".to_owned())
    );
    assert!(!lock(&jobs).jobs.contains_key("codex-1-0"));
}

#[test]
fn stop_all_closes_the_registry_and_stops_every_registered_job() {
    let root = tempfile::TempDir::new().expect("temp dir");
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("spawn sleep");
    let pid = Pid::from_raw(child.id() as i32);
    let jobs = jobs_with("codex-1-0", job_for(root.path(), pid, false));
    let spawner = CodexSummarySpawner {
        ctx: Weak::new(),
        counter: AtomicU64::new(0),
        jobs: Arc::clone(&jobs),
    };

    spawner.stop_all();

    {
        let registry = lock(&jobs);
        assert!(registry.closed);
        assert!(registry.jobs["codex-1-0"].stopping);
    }
    // SIGTERM ends `sleep`; the forced stop 5 s later finds the job gone.
    let status = child.wait().expect("reap sleep");
    assert!(!status.success());
    lock(&jobs).jobs.remove("codex-1-0");
    let late = job_for(root.path(), Pid::from_raw(i32::MAX), false);
    assert!(!register(&jobs, "codex-1-1", late));
    assert!(!lock(&jobs).jobs.contains_key("codex-1-1"));
}

#[test]
fn an_open_registry_accepts_a_job() {
    let root = tempfile::TempDir::new().expect("temp dir");
    let jobs = Jobs::default();
    let job = job_for(root.path(), Pid::from_raw(i32::MAX), false);
    assert!(register(&jobs, "codex-1-0", job));
    assert!(lock(&jobs).jobs.contains_key("codex-1-0"));
}
