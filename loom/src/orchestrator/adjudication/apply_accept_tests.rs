//! One accepted verdict patching two criteria of a fifteen-criterion stage:
//! both amendments apply against the list as filed, each once, and the next
//! session is told what changed and to complete first.

use std::path::{Path, PathBuf};

use super::*;
use crate::models::dispute::{
    applied_marker, Citation, CriterionField, DisputeKind, DisputeVerdict,
};
use crate::models::stage::{AcceptanceCriterion, StageStatus};
use crate::orchestrator::adjudication::tests::{
    make_stage, write_request, write_stage, write_verdict,
};
use crate::orchestrator::adjudication::{feedback, AdjudicatorRegistry};
use crate::plan::amendment::count_amendments_for_stage;
use crate::verify::transitions::load_stage;

const CRITERIA: usize = 15;

fn criterion(i: usize) -> String {
    format!("cargo test --lib case_{i}")
}

/// `root/.loom/work`, its `config.toml` naming a plan whose stage `s1` has
/// fifteen acceptance criteria. Returns the work dir and the plan path.
fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let plans = root.join("doc").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let acceptance: String = (0..CRITERIA)
        .map(|i| format!("        - \"{}\"\n", criterion(i)))
        .collect();
    let plan_text = format!(
        "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: s1\n      name: s1\n      working_dir: \".\"\n      acceptance:\n{acceptance}```\n\n\
         <!-- END loom METADATA -->\n"
    );
    let plan = plans.join("PLAN-two-criteria.md");
    std::fs::write(&plan, plan_text).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 1;
    stage.acceptance = (0..CRITERIA)
        .map(|i| AcceptanceCriterion::Simple(criterion(i)))
        .collect();
    write_stage(&work, &stage);
    (work, plan)
}

fn accept_replacing_13_and_deleting_14() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: serde_json::json!({
                "field": "acceptance",
                "patches": [
                    {"op": "replace", "index": 13, "value": "cargo test --lib portable"},
                    {"op": "delete", "index": 14},
                ],
                "reason": "13 needs the network; 14 needs git metadata",
            }),
        },
        citations: vec![Citation {
            file: "PLAN.md".to_string(),
            line: None,
            excerpt: "case_13".to_string(),
            claim: "the sandbox denies the network".to_string(),
        }],
        reasoning: "criterion 13 has a portable form; 14 runs nowhere completion runs, so \
                    integration-verify keeps that check"
            .to_string(),
    }
}

#[test]
fn an_accept_patching_two_criteria_amends_both_once_and_requeues_with_a_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![13, 14]);
    write_request(&work, "s1", 1, kind);
    write_verdict(&work, "s1", 1, accept_replacing_13_and_deleting_14(), 1);

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "s1", 1)
        .unwrap();

    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(stage.acceptance.len(), CRITERIA - 1);
    assert_eq!(
        stage.acceptance[13],
        AcceptanceCriterion::Simple("cargo test --lib portable".to_string())
    );
    assert_eq!(
        stage.acceptance[12],
        AcceptanceCriterion::Simple(criterion(12))
    );
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 2);
    let notice = feedback::read_feedback(&work, "s1").unwrap().unwrap();
    for expected in [
        "Dispute 1 accepted: acceptance 13 replaced, 14 deleted",
        "integration-verify keeps that check",
        "run `loom stage complete s1` before anything else",
    ] {
        assert!(notice.contains(expected), "missing {expected:?}: {notice}");
    }

    // An interrupted apply runs again: every patch is already recorded.
    let amended = std::fs::read(&plan).unwrap();
    std::fs::remove_file(applied_marker(&work.join("disputes"), "s1", 1)).unwrap();
    AdjudicatorRegistry::new()
        .apply_verdict(&work, "s1", 1)
        .unwrap();
    assert_eq!(std::fs::read(&plan).unwrap(), amended);
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 2);
    assert_eq!(
        load_stage("s1", &work).unwrap().acceptance.len(),
        CRITERIA - 1
    );
}

#[test]
fn the_notice_caps_the_reasoning() {
    let requests = build_amendment_requests(
        "s1",
        &PlanPatch {
            inner: serde_json::json!({"field": "wiring", "patch": {"op": "delete", "index": 2}}),
        },
        4,
    )
    .unwrap();

    let notice = accept_notice("s1", 4, &requests, &"r".repeat(5000));

    assert!(
        notice.starts_with("Dispute 4 accepted: wiring 2 deleted"),
        "{notice}"
    );
    assert!(notice.contains("[truncated]"), "{notice}");
    assert!(notice.len() < REASONING_MAX_BYTES + 400, "{}", notice.len());
}

/// File and apply dispute `id`, an accept replacing every criterion in `indices`.
fn apply_replacing(work: &Path, id: u32, indices: &[usize]) {
    let patches: Vec<_> = indices
        .iter()
        .map(|i| serde_json::json!({"op": "replace", "index": i, "value": format!("cargo test --lib fixed_{i}")}))
        .collect();
    let verdict = DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: serde_json::json!({"field": "acceptance", "patches": patches, "reason": "r"}),
        },
        citations: vec![Citation {
            file: "PLAN.md".to_string(),
            line: None,
            excerpt: "case".to_string(),
            claim: "c".to_string(),
        }],
        reasoning: "wrong criteria".to_string(),
    };
    crate::verify::transitions::update_stage("s1", work, |stage| {
        stage.status = StageStatus::NeedsAdjudication;
        stage.dispute_count = id;
        Ok(())
    })
    .unwrap();
    write_request(
        work,
        "s1",
        id,
        DisputeKind::criterion(CriterionField::Acceptance, indices.to_vec()),
    );
    write_verdict(work, "s1", id, verdict, 1);
    AdjudicatorRegistry::new()
        .apply_verdict(work, "s1", id)
        .unwrap();
}

#[test]
fn a_four_patch_verdict_counts_once_against_the_cap_and_applies_whole() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, _plan) = setup(tmp.path());

    apply_replacing(&work, 1, &[0, 1, 2, 3]);

    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 4);
    assert_eq!(stage.status, StageStatus::Queued);
    for i in 0..4 {
        assert_eq!(
            stage.acceptance[i],
            AcceptanceCriterion::Simple(format!("cargo test --lib fixed_{i}"))
        );
    }
}

#[test]
fn a_verdict_past_the_cap_applies_nothing_and_escalates() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    let text = std::fs::read_to_string(&plan).unwrap().replace(
        "  stages:\n",
        "  adjudication:\n    max_amendments_per_stage: 3\n  stages:\n",
    );
    std::fs::write(&plan, text).unwrap();
    apply_replacing(&work, 1, &[0, 1]);
    apply_replacing(&work, 2, &[2]);
    apply_replacing(&work, 3, &[3]);
    let before = std::fs::read(&plan).unwrap();

    apply_replacing(&work, 4, &[4, 5]);

    assert_eq!(std::fs::read(&plan).unwrap(), before);
    assert_eq!(count_amendments_for_stage(&work, "s1").unwrap(), 4);
    assert_eq!(
        load_stage("s1", &work).unwrap().status,
        StageStatus::NeedsHumanReview
    );
}
