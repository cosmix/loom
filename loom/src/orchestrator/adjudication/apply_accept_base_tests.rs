//! A narrowing rests on a base run only at the stage's current base, and an
//! amendment the cap refuses hands nothing off.

use super::replay_tests::write_narrowing;
use super::tests::{apply_patch, failing_base_run, narrowing_json, setup, NARROWED};
use crate::models::dispute::{verdict_file, CriterionField, DisputeKind};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::record::record_verdict_text;
use crate::orchestrator::adjudication::tests::write_request;
use crate::orchestrator::adjudication::{feedback, AdjudicatorRegistry};
use crate::plan::amendment::count_amendments_for_stage;
use crate::verify::review::handoffs::test_support::{advance_base, base_of};
use crate::verify::review::handoffs::{load_handoffs, save_base_evidence};
use crate::verify::transitions::load_stage;

#[test]
fn a_base_run_at_an_earlier_base_does_not_support_a_recorded_narrowing() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, _plan) = setup(tmp.path());
    let stale = failing_base_run(&work);
    save_base_evidence(&work, "s1", &stale).unwrap();
    let base = advance_base(tmp.path(), "s1");
    assert_ne!(stale.base, base);
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![NARROWED]);
    write_request(&work, "s1", 1, kind);
    let raw = serde_json::json!({
        "verdict": "accept",
        "reasoning": "the test needs the network",
        "citations": [{"file": ".", "excerpt": "exit 101", "claim": "fails"}],
        "plan_patch": narrowing_json(),
    })
    .to_string();

    let err = record_verdict_text(&work, "s1", 1, &raw, None).unwrap_err();

    let text = format!("{err:#}");
    assert!(
        text.contains("current base") && text.contains("--at-base"),
        "{text}"
    );
    assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());

    // The same failing run, made at the base the stage has now, supports it.
    save_base_evidence(&work, "s1", &failing_base_run(&work)).unwrap();
    assert_eq!(base_of(&work), base);
    record_verdict_text(&work, "s1", 1, &raw, None).unwrap();
}

#[test]
fn a_narrowing_whose_base_moved_before_it_applied_amends_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    save_base_evidence(&work, "s1", &failing_base_run(&work)).unwrap();
    write_narrowing(&work);
    advance_base(tmp.path(), "s1");
    let before = std::fs::read(&plan).unwrap();

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "s1", 1)
        .unwrap();

    assert!(load_handoffs(&work, "s1").unwrap().is_empty());
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 0);
    assert_eq!(std::fs::read(&plan).unwrap(), before);
    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(stage.tally.evidence_rounds, 1);
    let feedback = feedback::read_feedback(&work, "s1").unwrap().unwrap();
    assert!(feedback.contains("current base"), "{feedback}");
}

#[test]
fn a_narrowing_the_cap_refuses_leaves_no_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    let text = std::fs::read_to_string(&plan).unwrap().replace(
        "  stages:\n",
        "  adjudication:\n    max_amendments_per_stage: 1\n  stages:\n",
    );
    std::fs::write(&plan, text).unwrap();
    let replace_first = serde_json::json!({
        "field": "acceptance",
        "patches": [{"op": "replace", "index": 0, "value": "cargo test --lib fixed_0"}],
    });
    apply_patch(&work, 1, &[0], replace_first);
    save_base_evidence(&work, "s1", &failing_base_run(&work)).unwrap();
    let before = std::fs::read(&plan).unwrap();

    apply_patch(&work, 2, &[NARROWED], narrowing_json());

    assert_eq!(std::fs::read(&plan).unwrap(), before);
    assert_eq!(
        load_stage("s1", &work).unwrap().status,
        StageStatus::NeedsHumanReview
    );
    assert!(
        load_handoffs(&work, "s1").unwrap().is_empty(),
        "a refused amendment leaves no test out"
    );
}
