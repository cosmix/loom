//! Supervised scheduler exits and the terminal outcome each one publishes.

use super::*;

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
    fs::create_dir_all(root.join(".loom/memory")).unwrap();
    let blocker = root.join(".loom/memory/archive");
    fs::write(&blocker, "blocker").unwrap();
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
    (work, plan, blocker, result)
}

fn published_outcome(work: &WorkDir) -> TerminalOutcome {
    let marker = fs::read_to_string(work.root().join(COMPLETION_MARKER)).unwrap();
    serde_json::from_str(&marker).unwrap()
}

#[test]
fn archive_failure_is_published_as_finalization_failure() {
    let temp = tempfile::tempdir().unwrap();
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
}

#[test]
fn finalization_retries_to_success_and_repeating_it_moves_nothing_again() {
    let temp = tempfile::tempdir().unwrap();
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
