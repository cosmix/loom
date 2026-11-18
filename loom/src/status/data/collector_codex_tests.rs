use super::*;
use crate::status::data::codex_jobs::tests::{fixture, seed};

#[test]
fn the_summary_carries_the_resolved_effort_and_the_codex_jobs() {
    let (_temp, work) = fixture();
    seed(work.root(), "u", 1, &[]);
    let stage = Stage {
        id: "s1".into(),
        name: "S".into(),
        reasoning_effort: Some("medium".into()),
        ..Stage::default()
    };
    let summary = build_stage_summary(&stage, &[], &work);
    assert_eq!(summary.effort, "medium");
    assert_eq!(summary.codex_jobs.len(), 1);
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(json["effort"], "medium");
    assert_eq!(json["codex_jobs"][0]["unit_id"], "u");
    assert_eq!(json["codex_jobs"][0]["phase"], "admitted");
}
