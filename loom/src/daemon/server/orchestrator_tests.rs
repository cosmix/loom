//! Supervised scheduler exits and the terminal outcome each one publishes.

use super::*;
use crate::user_config::{redirect_user_config, UserConfigRedirect};

#[test]
fn initialization_errors_and_panics_retire_essential_worker() {
    for panic in [false, true] {
        let temp = tempfile::TempDir::new().unwrap();
        let shutdown = AtomicBool::new(false);
        supervise_worker(temp.path(), &shutdown, || {
            if panic {
                panic!("constructor panicked");
            }
            anyhow::bail!("graph initialization failed")
        });
        assert!(shutdown.load(Ordering::Acquire));
        assert!(temp.path().join("orchestrator.complete").exists());
        let failure = fs::read_to_string(temp.path().join("scheduler.failed")).unwrap();
        assert!(failure.contains(if panic {
            "constructor panicked"
        } else {
            "graph initialization failed"
        }));
        let outcome: TerminalOutcome = serde_json::from_str(
            &fs::read_to_string(temp.path().join("orchestrator.complete")).unwrap(),
        )
        .unwrap();
        assert!(matches!(outcome, TerminalOutcome::SchedulerFailure(_)));
        fs::create_dir(temp.path().join("stages")).unwrap();
        let summary = super::super::status::collect_completion_summary(temp.path()).unwrap();
        assert_eq!(
            summary.failure_count, 1,
            "no stage failed, the scheduler did"
        );
    }
}

#[test]
fn cyclic_graph_fails_at_supervised_initialization_boundary() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().join("stages");
    fs::create_dir_all(&dir).unwrap();
    for (id, dep) in [("a", "b"), ("b", "a")] {
        let stage = crate::models::stage::Stage {
            id: id.into(),
            dependencies: vec![dep.into()],
            ..Default::default()
        };
        fs::write(
            dir.join(format!("{id}.md")),
            crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap(),
        )
        .unwrap();
    }
    let shutdown = AtomicBool::new(false);
    supervise_worker(temp.path(), &shutdown, || {
        build_execution_graph(temp.path()).map(|_| TerminalOutcome::Success)
    });
    assert!(shutdown.load(Ordering::Acquire));
    assert!(temp.path().join("scheduler.failed").exists());
}

/// A merged run whose archive target is blocked by a file at `.loom/memory/archive`.
/// Returns the work directory, its plan, the blocker and the finished result.
fn run_with_blocked_archive(root: &Path) -> (WorkDir, PathBuf, PathBuf, OrchestratorResult) {
    let (work, plan, result) = merged_run(root);
    fs::create_dir_all(root.join(".loom/memory")).unwrap();
    let blocker = root.join(".loom/memory/archive");
    fs::write(&blocker, "blocker").unwrap();
    (work, plan, blocker, result)
}

/// A run whose one stage completed and merged. Returns the work directory,
/// its plan and the finished result.
fn merged_run(root: &Path) -> (WorkDir, PathBuf, OrchestratorResult) {
    let work = WorkDir::new(root).unwrap();
    work.initialize().unwrap();
    let plan = root.join("IN_PROGRESS-PLAN-test.md");
    fs::write(&plan, "# Test plan\n").unwrap();
    fs::write(
        work.root().join("config.toml"),
        format!(
            "[plan]\nsource_path = {:?}\nplan_id = \"test\"\n",
            plan.to_str().unwrap()
        ),
    )
    .unwrap();
    fs::create_dir_all(work.root().join("stages")).unwrap();
    let stage = crate::models::stage::Stage {
        id: "test".into(),
        name: "Test".into(),
        status: crate::models::stage::StageStatus::Completed,
        merged: true,
        ..Default::default()
    };
    fs::write(
        work.root().join("stages/test.md"),
        crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(work.root().join("memory")).unwrap();
    fs::write(work.root().join("memory/test.md"), "journal").unwrap();
    let now = chrono::Utc::now();
    let result = OrchestratorResult {
        completed_stages: vec!["test".into()],
        failed_stages: vec![],
        unfinished_stages: vec![],
        needs_handoff: vec![],
        total_sessions_spawned: 1,
        started_at: now,
        completed_at: now,
    };
    (work, plan, result)
}

/// Points the loom home, where plan statistics are archived, at `root/home`.
fn redirect_home(root: &Path) -> (PathBuf, UserConfigRedirect) {
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let redirect = redirect_user_config(home.join("config.toml"));
    (home, redirect)
}

fn published_outcome(work: &WorkDir) -> TerminalOutcome {
    let marker = fs::read_to_string(work.root().join(COMPLETION_MARKER)).unwrap();
    serde_json::from_str(&marker).unwrap()
}

#[test]
fn archive_failure_is_published_as_finalization_failure() {
    let temp = tempfile::tempdir().unwrap();
    let (home, _redirect) = redirect_home(temp.path());
    let (work, plan, _blocker, result) = run_with_blocked_archive(temp.path());
    let shutdown = AtomicBool::new(false);
    supervise_worker(work.root(), &shutdown, || {
        Ok(finish_orchestration(&work, &result))
    });
    assert!(matches!(
        published_outcome(&work),
        TerminalOutcome::FinalizationFailure(_)
    ));
    assert!(plan.exists());
    assert!(finish_foreground(&work, &result).is_err());
    let summary = super::super::status::collect_completion_summary(work.root()).unwrap();
    assert_eq!((summary.failure_count, summary.success_count), (1, 1));
    assert!(
        !home.join("stats").exists(),
        "a failed finalization publishes no plan statistics"
    );
}

#[test]
fn finalization_retries_to_success_and_repeating_it_moves_nothing_again() {
    let temp = tempfile::tempdir().unwrap();
    let (_home, _redirect) = redirect_home(temp.path());
    let (work, plan, blocker, result) = run_with_blocked_archive(temp.path());
    assert!(finish_foreground(&work, &result).is_err());
    assert!(
        plan.exists(),
        "a failed archive must leave the plan in progress"
    );

    fs::remove_file(&blocker).unwrap();
    finish_foreground(&work, &result).unwrap();
    assert_eq!(published_outcome(&work), TerminalOutcome::Success);
    assert!(!plan.exists());
    let done = temp.path().join("DONE-PLAN-test.md");
    assert!(done.exists());
    let summary = super::super::status::collect_completion_summary(work.root()).unwrap();
    assert_eq!(
        (summary.failure_count, summary.success_count),
        (0, 1),
        "{:?}",
        super::super::status::completion_diagnostics(work.root())
    );
    let archived = fs::read_dir(temp.path().join(".loom/memory/archive"))
        .unwrap()
        .count();

    finish_foreground(&work, &result).unwrap();
    assert!(done.exists());
    let after_repeat = fs::read_dir(temp.path().join(".loom/memory/archive"))
        .unwrap()
        .count();
    assert_eq!(after_repeat, archived, "a repeat must not archive twice");
}

fn archived_stats(home: &Path) -> usize {
    fs::read_dir(home.join("stats/plans")).map_or(0, Iterator::count)
}

#[test]
fn plan_statistics_publish_only_on_success_and_never_change_the_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let (home, _redirect) = redirect_home(temp.path());
    let (work, _plan, result) = merged_run(temp.path());
    let current = work.root().join("plan-stats.json");

    let failed = OrchestratorResult {
        completed_stages: vec![],
        failed_stages: vec!["test".into()],
        unfinished_stages: vec![],
        needs_handoff: vec![],
        total_sessions_spawned: 1,
        started_at: result.started_at,
        completed_at: result.completed_at,
    };
    assert!(matches!(
        finish_orchestration(&work, &failed),
        TerminalOutcome::Incomplete(_)
    ));
    assert!(!home.join("stats").exists(), "an incomplete run published");
    assert!(
        !current.exists(),
        "an incomplete run wrote the current copy"
    );

    // A file where the archive directory goes fails the publish.
    fs::write(home.join("stats"), "blocker").unwrap();
    assert_eq!(
        finish_orchestration(&work, &result),
        TerminalOutcome::Success,
        "a failed publish must not change the outcome"
    );
    assert!(!current.exists());

    fs::remove_file(home.join("stats")).unwrap();
    assert_eq!(
        finish_orchestration(&work, &result),
        TerminalOutcome::Success
    );
    assert_eq!(archived_stats(&home), 1);
    assert!(
        current.is_file(),
        "a published run exposes the current copy"
    );
}

/// One assistant line of `msg_id` with `cache_write` tokens, as a Claude
/// transcript holds it.
fn transcript_line(msg_id: &str, cache_write: u64) -> String {
    let at = chrono::Utc::now() - chrono::Duration::seconds(60);
    let line = serde_json::json!({
        "type": "assistant", "timestamp": at.to_rfc3339(), "isSidechain": false,
        "sessionId": "01ed9f4c-608e-4e2c-8edf-05d26adcd4d6",
        "message": {"id": msg_id, "model": "claude-opus-5-5", "usage": {
            "input_tokens": 0, "cache_creation_input_tokens": cache_write,
            "cache_read_input_tokens": 0, "output_tokens": 0}}
    });
    format!("{line}\n")
}

/// A line written after the tracker's last checkpoint reaches the published
/// statistics only through the final scan `finish_orchestration` runs.
#[test]
fn published_plan_statistics_count_a_line_written_after_the_last_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let (_home, _redirect) = redirect_home(temp.path());
    let (work, _plan, result) = merged_run(temp.path());
    let transcript = temp.path().join("transcript.jsonl");
    fs::write(&transcript, transcript_line("m1", 100)).unwrap();
    let mut session = crate::models::session::Session::new();
    session.stage_id = Some("test".into());
    session.transcript_path = Some(transcript.to_string_lossy().into_owned());
    crate::fs::session_files::save_session(&session, work.root()).unwrap();
    let now = chrono::Utc::now().timestamp();
    crate::tokens::scan_pass(work.root(), None, now).unwrap();
    let checkpoint = work
        .root()
        .join("tokens")
        .join(crate::tokens::CHECKPOINT_FILE);
    assert!(
        checkpoint.is_file(),
        "the tracker pass wrote its checkpoint"
    );
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap();
    std::io::Write::write_all(&mut file, transcript_line("m2", 7).as_bytes()).unwrap();

    assert_eq!(
        finish_orchestration(&work, &result),
        TerminalOutcome::Success
    );
    let current = fs::read_to_string(work.root().join("plan-stats.json")).unwrap();
    let record: serde_json::Value = serde_json::from_str(&current).unwrap();
    assert_eq!(record["totals"]["cache_write"], 107, "{record}");
}
