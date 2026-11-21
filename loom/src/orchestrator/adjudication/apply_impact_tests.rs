//! Applying impact-test verdicts through `AdjudicatorRegistry::apply_verdict`,
//! from the files the daemon reads: the stage, `request.md` and `verdict.md`.

use serde_json::json;
use tempfile::TempDir;

use super::super::tests::{make_stage, reject_verdict, write_request, write_stage, write_verdict};
use super::super::{feedback, AdjudicatorRegistry};
use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::StageStatus;
use crate::testrun::TestTarget;
use crate::verify::impact_tests::{load_accepted, selection_digest, FailureKind, ImpactFailure};
use crate::verify::transitions::load_stage;

const ID: &str = "IT-pytest:deps/kit";

fn file_target(file: &str) -> TestTarget {
    TestTarget {
        file: file.to_string(),
        name: None,
    }
}

fn failure() -> ImpactFailure {
    ImpactFailure {
        failed_tests: Vec::new(),
        id: ID.to_string(),
        command: "uv run pytest tests/test_kit.py".to_string(),
        tests: vec!["tests/test_kit.py".to_string()],
        selection_digest: selection_digest(&[file_target("tests/test_kit.py")]),
        kind: FailureKind::DoesNotBuild,
        base: "the stage's base 0123abcd proved nothing".to_string(),
        accepted: None,
        output: "ModuleNotFoundError: No module named 'kit_native'".to_string(),
    }
}

/// Dispute 1 of stage `s1` contests [`ID`], and its verdict is `verdict`.
fn dispute(work: &std::path::Path, verdict: DisputeVerdict) {
    dispute_of(work, failure(), verdict);
}

/// Dispute 1 of stage `s1` contests `failure` as evidence, judged by `verdict`.
fn dispute_of(work: &std::path::Path, failure: ImpactFailure, verdict: DisputeVerdict) {
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
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

fn accept() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: json!({}) },
        citations: vec![Citation {
            file: "deps/kit/setup.py".to_string(),
            line: Some(3),
            excerpt: "ext_modules=[native]".to_string(),
            claim: "the native module never builds offline".to_string(),
        }],
        reasoning: "the stage never touches the vendored kit".to_string(),
    }
}

#[test]
fn an_accepted_impact_dispute_records_the_failure_as_judged() {
    let tmp = TempDir::new().unwrap();

    dispute(tmp.path(), accept());

    let accepted = load_accepted(tmp.path(), "s1").unwrap().accepted;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].id, ID);
    assert_eq!(accepted[0].kind, FailureKind::DoesNotBuild);
    assert_eq!(accepted[0].tests, vec!["tests/test_kit.py".to_string()]);
    assert_eq!(
        accepted[0].selection_digest,
        selection_digest(&[file_target("tests/test_kit.py")])
    );
    assert_eq!(accepted[0].dispute, 1);
    assert_eq!(
        load_stage("s1", tmp.path()).unwrap().status,
        StageStatus::NeedsAdjudication
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

/// Evidence over 721 tests lists 64 names; the accepted record keeps those and
/// the digest of all 721, which is what the gate matches.
#[test]
fn an_accepted_impact_dispute_over_721_tests_keeps_the_whole_selection_digest() {
    let tmp = TempDir::new().unwrap();
    let targets: Vec<TestTarget> = (0..721)
        .map(|n| file_target(&format!("tests/test_{n:03}.py")))
        .collect();
    let evidence = ImpactFailure {
        tests: targets.iter().take(64).map(|t| t.file.clone()).collect(),
        selection_digest: selection_digest(&targets),
        ..failure()
    };

    dispute_of(tmp.path(), evidence, accept());

    let accepted = load_accepted(tmp.path(), "s1").unwrap().accepted;
    assert_eq!(accepted[0].tests.len(), 64);
    assert_eq!(accepted[0].selection_digest, selection_digest(&targets));
}
