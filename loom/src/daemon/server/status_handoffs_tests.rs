//! The hand-offs and the amendment count a plan's summary carries.

use super::*;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::review::handoffs::{record_handoffs, Handoff};

const AUDIT: &str = "# Plan Amendment Audit\n\n\
| version | stage_id | field | op | index | applied_at | dispute_id | reason |\n\
|---------|----------|-------|----|-------|------------|------------|--------|\n\
| 1 | alpha | acceptance | replace | 0 | 2026-01-01T00:00:00+00:00 | d1 | flag renamed |\n\
| 2 | beta | contracts | delete | 1 | 2026-01-01T00:05:00+00:00 | d2 | contract retired |\n";

fn finished(id: &str) -> StageCompletionInfo {
    let mut stage = Stage::new(id.to_string(), None);
    stage.id = id.to_string();
    stage.status = StageStatus::Completed;
    completion_info(stage)
}

fn handoff(stage: &str, test: &str, verified: bool) -> Handoff {
    Handoff {
        stage: stage.to_string(),
        field: "acceptance".to_string(),
        index: 0,
        tests: vec![test.to_string()],
        runs_at: "ci".to_string(),
        reason: "needs network".to_string(),
        dispute: format!("dispute-{stage}"),
        verified,
    }
}

fn write_audit(work_dir: &Path, body: &str) {
    let dir = work_dir.join("plan_versions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("audit.md"), body).unwrap();
}

#[test]
fn the_summary_returns_the_recorded_handoffs_and_amendment_count() {
    let temp = tempfile::tempdir().unwrap();
    record_handoffs(temp.path(), "alpha", &[handoff("alpha", "net_a", true)]).unwrap();
    record_handoffs(temp.path(), "beta", &[handoff("beta", "net_b", false)]).unwrap();
    // A stage outside the summary's stages contributes nothing.
    record_handoffs(temp.path(), "other", &[handoff("other", "net_c", true)]).unwrap();
    write_audit(temp.path(), AUDIT);
    let mut errors = Vec::new();

    let (handoffs, amendments) = completion_handoffs(
        temp.path(),
        &[finished("alpha"), finished("beta")],
        &mut errors,
    );

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(amendments, 2);
    assert_eq!(handoffs.len(), 2, "{handoffs:?}");
    assert_eq!(
        handoffs[0],
        "alpha acceptance[0]: net_a -> ci: needs network"
    );
    assert_eq!(
        handoffs[1],
        "beta acceptance[0]: net_b -> ci: needs network (unverified)"
    );
}

#[test]
fn the_summary_is_empty_without_records() {
    let temp = tempfile::tempdir().unwrap();
    let mut errors = Vec::new();

    let (handoffs, amendments) =
        completion_handoffs(temp.path(), &[finished("alpha")], &mut errors);

    assert!(
        handoffs.is_empty() && amendments == 0 && errors.is_empty(),
        "{errors:?}"
    );
}

#[test]
fn an_unreadable_amendment_journal_is_a_diagnostic_not_an_empty_count() {
    let temp = tempfile::tempdir().unwrap();
    write_audit(temp.path(), "not an audit table\n");
    let mut errors = Vec::new();

    let (_, amendments) = completion_handoffs(temp.path(), &[finished("alpha")], &mut errors);

    assert_eq!(amendments, 0);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("amendment journal"), "{errors:?}");
}
