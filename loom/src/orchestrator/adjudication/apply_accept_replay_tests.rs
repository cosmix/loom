//! Hand-offs across an interrupted or repeated apply: they are recorded before
//! the amendment, so a replay neither loses nor misattributes one, and an
//! apply-time narrowing that no longer checks amends nothing, hands nothing
//! off and asks the agent for a fresh base run.

use std::path::Path;

use super::tests::{
    accept_replacing_13_and_deleting_14, criterion, failing_base_run, narrowing_json, setup,
    NARROWED,
};
use super::*;
use crate::models::dispute::{applied_marker, CriterionField, DisputeKind, DisputeVerdict};
use crate::models::stage::AcceptanceCriterion;
use crate::orchestrator::adjudication::tests::{write_request, write_verdict};
use crate::orchestrator::adjudication::{feedback, AdjudicatorRegistry};
use crate::plan::amendment::count_amendments_for_stage;
use crate::plan::schema::ContractSpec;
use crate::verify::review::handoffs::{load_handoffs, save_base_evidence, BaseEvidence};
use crate::verify::transitions::load_stage;

/// The accepted narrowing of criterion [`NARROWED`], written as dispute 1 of `s1`.
fn write_narrowing(work: &Path) {
    let DisputeVerdict::Accept {
        citations,
        reasoning,
        ..
    } = accept_replacing_13_and_deleting_14()
    else {
        unreachable!("the fixture is an accept");
    };
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![NARROWED]);
    write_request(work, "s1", 1, kind);
    let verdict = DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: narrowing_json(),
        },
        citations,
        reasoning,
    };
    write_verdict(work, "s1", 1, verdict, 1);
}

/// Run `apply` with `plan` moved away, so the amendment fails after the
/// hand-offs are written, then put it back.
fn with_plan_missing(plan: &Path, apply: impl FnOnce() -> anyhow::Result<()>) {
    let aside = plan.with_extension("aside");
    std::fs::rename(plan, &aside).unwrap();
    let outcome = apply();
    std::fs::rename(&aside, plan).unwrap();
    assert!(outcome.is_err(), "the amendment must fail without its plan");
}

#[test]
fn a_handoff_recorded_before_an_interrupted_amendment_survives_the_replay() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    save_base_evidence(&work, "s1", &failing_base_run()).unwrap();
    write_narrowing(&work);
    let apply = || AdjudicatorRegistry::new().apply_verdict(&work, "s1", 1);

    with_plan_missing(&plan, apply);

    let recorded = load_handoffs(&work, "s1").unwrap();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 0);
    apply().unwrap();
    assert_eq!(load_handoffs(&work, "s1").unwrap(), recorded);
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 1);
}

#[test]
fn a_replay_after_the_amendment_keeps_the_recorded_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, _plan) = setup(tmp.path());
    save_base_evidence(&work, "s1", &failing_base_run()).unwrap();
    write_narrowing(&work);
    let apply = || AdjudicatorRegistry::new().apply_verdict(&work, "s1", 1);
    apply().unwrap();
    let recorded = load_handoffs(&work, "s1").unwrap();
    assert_eq!(recorded.len(), 1, "{recorded:?}");

    // The amended criterion no longer checks: its replace equals the list now.
    std::fs::remove_file(applied_marker(&work.join("disputes"), "s1", 1)).unwrap();
    apply().unwrap();

    assert_eq!(load_handoffs(&work, "s1").unwrap(), recorded);
}

#[test]
fn a_narrowing_whose_base_run_now_passes_amends_nothing_and_asks_for_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    write_narrowing(&work);
    let passing = BaseEvidence {
        passed: true,
        failed: Some(0),
        ..failing_base_run()
    };
    save_base_evidence(&work, "s1", &passing).unwrap();
    let before = std::fs::read(&plan).unwrap();

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "s1", 1)
        .unwrap();

    assert!(load_handoffs(&work, "s1").unwrap().is_empty());
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 0);
    assert_eq!(std::fs::read(&plan).unwrap(), before);
    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(
        stage.acceptance[NARROWED],
        AcceptanceCriterion::Simple(criterion(NARROWED))
    );
    assert_eq!(stage.tally.evidence_rounds, 1);
    let feedback = feedback::read_feedback(&work, "s1").unwrap().unwrap();
    for expected in ["passes at the base", "--at-base", "Dispute 1"] {
        assert!(
            feedback.contains(expected),
            "missing {expected:?}: {feedback}"
        );
    }
}

#[test]
fn a_contract_deletion_replayed_after_the_amendment_names_no_shifted_contract() {
    use crate::models::dispute::ContractRulingKind::Accept;
    use crate::orchestrator::adjudication::contract_verdict_fixtures::{
        contract_kind, rulings_verdict,
    };
    use crate::verify::contracts::test_support::CONTRACT_ID;
    use crate::verify::review::handoffs::HANDOFFS_FILE;

    let fx = two_contracts();
    let patch =
        serde_json::json!({"field": "contracts", "patches": [{"op": "delete", "index": 0}]});
    write_request(&fx.work, "s1", 1, contract_kind(&[CONTRACT_ID], Vec::new()));
    write_verdict(
        &fx.work,
        "s1",
        1,
        rulings_verdict(&[(CONTRACT_ID, Accept)], patch),
        1,
    );
    let apply = || AdjudicatorRegistry::new().apply_verdict(&fx.work, "s1", 1);
    apply().unwrap();
    let handoffs = fx.work.join("reviews").join("s1").join(HANDOFFS_FILE);
    assert!(handoffs.exists(), "the first apply records the deletion");

    // A record lost after the amendment: index 0 now holds `rejects-y`.
    std::fs::remove_file(&handoffs).unwrap();
    std::fs::remove_file(applied_marker(&fx.work.join("disputes"), "s1", 1)).unwrap();
    apply().unwrap();

    let recorded = load_handoffs(&fx.work, "s1").unwrap();
    assert!(
        recorded
            .iter()
            .all(|handoff| handoff.tests != ["rejects-y"]),
        "{recorded:?}"
    );
}

#[test]
fn a_contract_deletion_is_handed_off_before_its_amendment() {
    use crate::models::dispute::ContractRulingKind::Accept;
    use crate::orchestrator::adjudication::contract_verdict_fixtures::{
        contract_kind, rulings_verdict,
    };
    use crate::verify::contracts::test_support::CONTRACT_ID;

    let fx = two_contracts();
    let plan = crate::fs::resolve_source_path(&fx.work).unwrap().unwrap();
    let patch =
        serde_json::json!({"field": "contracts", "patches": [{"op": "delete", "index": 0}]});
    write_request(&fx.work, "s1", 1, contract_kind(&[CONTRACT_ID], Vec::new()));
    write_verdict(
        &fx.work,
        "s1",
        1,
        rulings_verdict(&[(CONTRACT_ID, Accept)], patch),
        1,
    );
    let apply = || AdjudicatorRegistry::new().apply_verdict(&fx.work, "s1", 1);

    with_plan_missing(&plan, apply);

    let recorded = load_handoffs(&fx.work, "s1").unwrap();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0].tests, vec![CONTRACT_ID.to_string()]);
    apply().unwrap();
    assert_eq!(load_handoffs(&fx.work, "s1").unwrap(), recorded);
}

/// The contract fixture with a second contract after [`CONTRACT_ID`] in the
/// plan and the stage: a v2 stage keeps at least one contract.
fn two_contracts() -> crate::orchestrator::adjudication::apply_contract_tests::Fixture {
    use crate::verify::contracts::test_support::contract;
    let fx = crate::orchestrator::adjudication::apply_contract_tests::fixture(Some("."));
    let plan = crate::fs::resolve_source_path(&fx.work).unwrap().unwrap();
    let second = "        - id: rejects-y\n          file: \"tests/x_contract.rs\"\n          \
                  test: \"tests::rejects_y\"\n          runner: cargo-test\n          \
                  scenario: \"y arrives\"\n          rejects: \"accepting y\"\n";
    let text = std::fs::read_to_string(&plan).unwrap().replacen(
        "      acceptance:\n",
        &format!("{second}      acceptance:\n"),
        1,
    );
    std::fs::write(&plan, text).unwrap();
    crate::verify::transitions::update_stage("s1", &fx.work, |stage| {
        stage.contracts.push(ContractSpec {
            id: "rejects-y".to_string(),
            test: Some("tests::rejects_y".to_string()),
            scenario: "y arrives".to_string(),
            rejects: "accepting y".to_string(),
            ..contract()
        });
        Ok(())
    })
    .unwrap();
    fx
}

#[test]
fn a_deleted_contract_is_handed_off_once() {
    use crate::models::dispute::ContractRulingKind::Accept;
    use crate::orchestrator::adjudication::contract_verdict_fixtures::{
        contract_kind, rulings_verdict,
    };
    use crate::verify::contracts::test_support::CONTRACT_ID;
    use crate::verify::review::handoffs::Handoff;

    let fx = two_contracts();
    let patch =
        serde_json::json!({"field": "contracts", "patches": [{"op": "delete", "index": 0}]});
    write_request(&fx.work, "s1", 1, contract_kind(&[CONTRACT_ID], Vec::new()));
    let verdict = rulings_verdict(&[(CONTRACT_ID, Accept)], patch);
    write_verdict(&fx.work, "s1", 1, verdict, 1);
    let apply = || AdjudicatorRegistry::new().apply_verdict(&fx.work, "s1", 1);

    apply().unwrap();

    let expected = vec![Handoff {
        stage: "s1".to_string(),
        field: "contracts".to_string(),
        index: 0,
        tests: vec![CONTRACT_ID.to_string()],
        runs_at: "nowhere".to_string(),
        reason: format!("x arrives (deleted: judged {CONTRACT_ID} on the cited lines)"),
        dispute: "1".to_string(),
        verified: false,
    }];
    assert_eq!(load_handoffs(&fx.work, "s1").unwrap(), expected);
    std::fs::remove_file(applied_marker(&fx.work.join("disputes"), "s1", 1)).unwrap();
    apply().unwrap();
    assert_eq!(load_handoffs(&fx.work, "s1").unwrap(), expected);
}
