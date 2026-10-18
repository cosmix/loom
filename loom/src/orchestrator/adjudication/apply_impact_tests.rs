//! Applying impact-test verdicts through `AdjudicatorRegistry::apply_verdict`,
//! from the files the daemon reads: the stage, `request.md` and `verdict.md`.

use serde_json::json;
use tempfile::TempDir;

use super::super::tests::{make_stage, reject_verdict, write_request, write_stage, write_verdict};
use super::super::{feedback, AdjudicatorRegistry};
use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::StageStatus;
use crate::verify::impact_tests::{load_accepted, FailureKind, ImpactFailure};
use crate::verify::transitions::load_stage;

const ID: &str = "IT-pytest:deps/kit";

/// Dispute 1 of stage `s1` contests [`ID`], and its verdict is `verdict`.
fn dispute(work: &std::path::Path, verdict: DisputeVerdict) {
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    let failure = ImpactFailure {
        id: ID.to_string(),
        command: "uv run pytest tests/test_kit.py".to_string(),
        tests: vec!["tests/test_kit.py".to_string()],
        kind: FailureKind::DoesNotBuild,
        base: "the stage's base 0123abcd proved nothing".to_string(),
        accepted: None,
        output: "ModuleNotFoundError: No module named 'kit_native'".to_string(),
    };
    let kind = DisputeKind::ImpactTests {
        test_ids: vec![ID.to_string()],
        evidence: vec![failure],
    };
    write_request(work, "s1", 1, kind);
    write_verdict(work, "s1", 1, verdict, 1);
    AdjudicatorRegistry::new()
        .apply_verdict(work, "s1", 1)
        .unwrap();
}

#[test]
fn an_accepted_impact_dispute_records_the_failure_as_judged() {
    let tmp = TempDir::new().unwrap();
    let accept = DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: json!({}) },
        citations: vec![Citation {
            file: "deps/kit/setup.py".to_string(),
            line: Some(3),
            excerpt: "ext_modules=[native]".to_string(),
            claim: "the native module never builds offline".to_string(),
        }],
        reasoning: "the stage never touches the vendored kit".to_string(),
    };

    dispute(tmp.path(), accept);

    let accepted = load_accepted(tmp.path(), "s1").unwrap().accepted;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].id, ID);
    assert_eq!(accepted[0].kind, FailureKind::DoesNotBuild);
    assert_eq!(accepted[0].tests, vec!["tests/test_kit.py".to_string()]);
    assert_eq!(accepted[0].dispute, 1);
    assert_eq!(
        load_stage("s1", tmp.path()).unwrap().status,
        StageStatus::Queued
    );
    let notice = feedback::read_feedback(tmp.path(), "s1").unwrap().unwrap();
    assert!(
        notice.contains("accepted your impact-test dispute"),
        "{notice}"
    );
}

#[test]
fn a_rejected_impact_dispute_records_nothing_and_says_the_failures_stand() {
    let tmp = TempDir::new().unwrap();

    dispute(tmp.path(), reject_verdict());

    assert!(load_accepted(tmp.path(), "s1").unwrap().accepted.is_empty());
    assert_eq!(
        load_stage("s1", tmp.path()).unwrap().status,
        StageStatus::Queued
    );
    let notice = feedback::read_feedback(tmp.path(), "s1").unwrap().unwrap();
    assert!(notice.contains(&format!("{ID} stand")), "{notice}");
}
