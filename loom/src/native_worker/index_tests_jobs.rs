use super::super::model::Outcome;
use super::tests::{admit, manifest};
use super::*;
use tempfile::TempDir;

#[test]
fn stage_jobs_folds_a_terminal_unit_with_its_end_time() {
    let work = TempDir::new().unwrap();
    let events = [
        NativeEvent::ThreadStarted {
            thread_id: "thread".into(),
        },
        NativeEvent::Terminal {
            outcome: Outcome::Succeeded,
            reason: "turn completed".into(),
        },
    ];
    let dir = admit(work.path(), "unit", 1, 0, &events);
    let recorded = read_events(&dir).unwrap();
    let jobs = jobs_in(&load_stage(work.path(), "stage"));
    let expected = NativeJob {
        unit_id: "unit".into(),
        invocation_id: format!("inv-{:032x}", 1),
        model: "gpt-5.4".into(),
        effort: "high".into(),
        phase: Phase::Terminal,
        outcome: Some(Outcome::Succeeded),
        outcome_reason: Some("turn completed".into()),
        thread_id: Some("thread".into()),
        started_at: manifest("unit", 1, 0).created_at,
        ended_at: Some(recorded[2].at),
        deadline_secs: manifest("unit", 1, 0).deadline_secs,
    };
    assert_eq!(jobs, [expected]);
}

#[test]
fn stage_jobs_lists_a_failed_unit_that_never_got_a_thread() {
    let work = TempDir::new().unwrap();
    let failed = NativeEvent::Terminal {
        outcome: Outcome::Failed,
        reason: "spawn failed".into(),
    };
    admit(work.path(), "late", 2, 20, &[failed]);
    admit(work.path(), "early", 1, 10, &[]);
    let bad = admit(work.path(), "corrupt", 3, 30, &[]);
    fs::write(bad.join("events.jsonl"), b"broken").unwrap();
    let jobs = jobs_in(&load_stage(work.path(), "stage"));
    let units: Vec<_> = jobs.iter().map(|job| job.unit_id.as_str()).collect();
    assert_eq!(units, ["early", "late"], "oldest first, corrupt skipped");
    assert_eq!(jobs[0].phase, Phase::Admitted);
    assert_eq!(jobs[0].ended_at, None);
    assert_eq!(jobs[1].outcome, Some(Outcome::Failed));
    assert_eq!(jobs[1].thread_id, None);
    assert!(jobs[1].ended_at.is_some());
}
