use super::*;
use crate::models::stage::Stage;
use crate::verify::transitions::save_stage;
use tempfile::TempDir;

fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom/work");
    fs::create_dir_all(work.join("stages")).unwrap();
    let plan = temp.path().join("plan.md");
    let mut stages = String::new();
    for id in ["a", "b"] {
        stages.push_str(&format!("    - id: {id}\n      name: {id}\n      working_dir: .\n      acceptance: [one, two]\n"));
        let stage = Stage {
            id: id.into(),
            name: id.into(),
            working_dir: Some(".".into()),
            acceptance: vec![
                AcceptanceCriterion::Simple("one".into()),
                AcceptanceCriterion::Simple("two".into()),
            ],
            ..Stage::default()
        };
        save_stage(&stage, &work).unwrap();
    }
    fs::write(&plan, format!("# Plan\n<!-- loom METADATA -->\n```yaml\nloom:\n  version: 1\n  stages:\n{stages}```\n<!-- END loom METADATA -->\n")).unwrap();
    (temp, work, plan)
}

fn request(stage: &str, patch: AmendmentPatch) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: stage.into(),
        field: AmendmentField::Acceptance,
        patch,
        reason: None,
        dispute_id: Some(format!("dispute-{stage}")),
    }
}

fn assert_policy(plan: &Path, work: &Path, stage_id: &str, expected: &[&str]) {
    let content = fs::read_to_string(plan).unwrap();
    let extracted = extract_yaml_metadata_with_ranges(&content).unwrap();
    let metadata = parse_and_validate(&extracted.yaml).unwrap();
    let definition = metadata
        .loom
        .stages
        .iter()
        .find(|s| s.id == stage_id)
        .unwrap();
    let expected: Vec<_> = expected
        .iter()
        .map(|s| AcceptanceCriterion::Simple((*s).into()))
        .collect();
    assert_eq!(definition.acceptance, expected);
    assert_eq!(load_stage(stage_id, work).unwrap().acceptance, expected);
}

fn interrupted_retry(phase: &'static str, patch: AmendmentPatch, expected: &[&str]) {
    let (_temp, work, plan) = fixture();
    let req = request("a", patch);
    transaction::fail_once(&work, phase);
    assert!(apply_amendment(&plan, &work, req.clone()).is_err());
    assert!(!plan_versions_dir(&work).join("1.applied").exists());
    assert_eq!(load_stage("a", &work).unwrap().acceptance.len(), 2);
    let result = apply_amendment(&plan, &work, req.clone()).unwrap();
    assert_eq!(result.version, 1);
    assert_policy(&plan, &work, "a", expected);
    assert!(plan_versions_dir(&work).join("1.applied").exists());
    apply_amendment(&plan, &work, req).unwrap();
    assert_policy(&plan, &work, "a", expected);
    assert_eq!(read_audit_rows(&work).unwrap().len(), 1);
}

#[test]
fn retry_after_audit_finishes_insert_snapshot_once() {
    interrupted_retry(
        "after-audit",
        AmendmentPatch::Insert {
            index: 0,
            value: "inserted".into(),
        },
        &["inserted", "one", "two"],
    );
}

#[test]
fn retry_after_plan_finishes_delete_snapshot_once() {
    interrupted_retry("after-plan", AmendmentPatch::Delete { index: 0 }, &["two"]);
}

#[test]
fn intervening_stage_amendment_finishes_pending_snapshot_without_losing_either() {
    let (_temp, work, plan) = fixture();
    let a = request(
        "a",
        AmendmentPatch::Insert {
            index: 0,
            value: "inserted".into(),
        },
    );
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    let b = request("b", AmendmentPatch::Delete { index: 0 });
    assert_eq!(apply_amendment(&plan, &work, b).unwrap().version, 2);
    assert_eq!(apply_amendment(&plan, &work, a).unwrap().version, 1);
    assert_policy(&plan, &work, "a", &["inserted", "one", "two"]);
    assert_policy(&plan, &work, "b", &["two"]);
    assert_eq!(read_audit_rows(&work).unwrap().len(), 2);
}

#[test]
fn failed_stage_write_cannot_create_receipt_or_return_idempotent_success() {
    let (_temp, work, plan) = fixture();
    let a = request("a", AmendmentPatch::Delete { index: 0 });
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    let stages = work.join("stages");
    fs::rename(&stages, work.join("saved-stages")).unwrap();
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    assert!(!plan_versions_dir(&work).join("1.applied").exists());
    fs::remove_dir_all(&stages).ok();
    fs::rename(work.join("saved-stages"), &stages).unwrap();
    apply_amendment(&plan, &work, a).unwrap();
    assert_policy(&plan, &work, "a", &["two"]);
}

fn accepted_verdict(work: &Path, plan: &Path) -> PathBuf {
    use crate::models::dispute::{DisputeVerdict, DisputeVerdictRecord, PlanPatch};
    fs::write(work.join("config.toml"), format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\nbase_branch = \"main\"\n", plan.display(),
    )).unwrap();
    update_stage("a", work, |stage| {
        stage.status = crate::models::stage::StageStatus::NeedsAdjudication;
        stage.completed_commit = Some("old-policy-commit".into());
        Ok(())
    })
    .unwrap();
    let record = DisputeVerdictRecord {
        id: 1,
        stage_id: "a".into(),
        adjudicator_attempt_count: 1,
        created_at: Utc::now(),
        model: "test".into(),
        session_id: None,
        verdict: DisputeVerdict::Accept {
            plan_patch: PlanPatch {
                inner: serde_json::json!({
                    "field": "acceptance", "op": "delete", "index": 0,
                }),
            },
            citations: vec![],
            reasoning: "remove incorrect criterion".into(),
        },
    };
    let dir = work.join("disputes/a/1");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("verdict.md"),
        format!("---\n{}---\n", crate::yaml::to_string(&record).unwrap()),
    )
    .unwrap();
    dir.join("applied.marker")
}

#[test]
fn adjudication_marker_waits_for_both_live_writes_on_retry() {
    use crate::orchestrator::adjudication::AdjudicatorRegistry;
    for phase in ["after-audit", "after-plan"] {
        let (_temp, work, plan) = fixture();
        let applied = accepted_verdict(&work, &plan);
        let registry = AdjudicatorRegistry::new();
        transaction::fail_once(&work, phase);
        assert!(registry.apply_verdict(&work, "a", 1).is_err());
        assert!(!applied.exists());
        assert_eq!(load_stage("a", &work).unwrap().acceptance.len(), 2);
        registry.apply_verdict(&work, "a", 1).unwrap();
        assert_policy(&plan, &work, "a", &["two"]);
        assert!(applied.exists());
        assert!(load_stage("a", &work).unwrap().completed_commit.is_none());
    }
}

#[test]
fn retry_of_older_unfinished_row_preserves_newer_unrelated_stage() {
    let (_temp, work, plan) = fixture();
    let original = fs::read_to_string(&plan).unwrap();
    let a = request("a", AmendmentPatch::Delete { index: 0 });
    apply_amendment(&plan, &work, a.clone()).unwrap();
    let b = request(
        "b",
        AmendmentPatch::Replace {
            index: 0,
            value: "new-b".into(),
        },
    );
    apply_amendment(&plan, &work, b).unwrap();
    // Reconstruct an older process's partial write, followed by another stage's commit.
    fs::remove_file(plan_versions_dir(&work).join("1.applied")).unwrap();
    amendment_catch_up::reconcile_amended_field(
        &plan,
        &resolve_project_root(&work, &plan),
        &original,
        "a",
        AmendmentField::Acceptance,
    )
    .unwrap();
    update_stage("a", &work, |stage| {
        stage.acceptance = vec![
            AcceptanceCriterion::Simple("one".into()),
            AcceptanceCriterion::Simple("two".into()),
        ];
        Ok(())
    })
    .unwrap();
    assert_eq!(apply_amendment(&plan, &work, a).unwrap().version, 1);
    assert_policy(&plan, &work, "a", &["two"]);
    assert_policy(&plan, &work, "b", &["new-b", "two"]);
    assert_eq!(read_audit_rows(&work).unwrap().len(), 2);
}

#[test]
fn live_plan_write_failure_remains_pending_until_retry() {
    let (_temp, work, plan) = fixture();
    let req = request("a", AmendmentPatch::Delete { index: 0 });
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, req.clone()).is_err());
    let saved = plan.with_extension("saved");
    fs::rename(&plan, &saved).unwrap();
    fs::create_dir(&plan).unwrap();
    assert!(apply_amendment(&plan, &work, req.clone()).is_err());
    assert!(!plan_versions_dir(&work).join("1.applied").exists());
    assert_eq!(load_stage("a", &work).unwrap().acceptance.len(), 2);
    fs::remove_dir(&plan).unwrap();
    fs::rename(saved, &plan).unwrap();
    assert_eq!(apply_amendment(&plan, &work, req).unwrap().version, 1);
    assert_policy(&plan, &work, "a", &["two"]);
}

#[test]
fn older_pending_receipt_cannot_revert_newer_same_field_amendment() {
    let (_temp, work, plan) = fixture();
    let first = request("a", AmendmentPatch::Delete { index: 0 });
    apply_amendment(&plan, &work, first.clone()).unwrap();
    let mut second = request(
        "a",
        AmendmentPatch::Insert {
            index: 0,
            value: "new".into(),
        },
    );
    second.dispute_id = Some("second-a".into());
    apply_amendment(&plan, &work, second).unwrap();
    // A legacy or interrupted journal can contain an older pending row.
    fs::remove_file(plan_versions_dir(&work).join("1.applied")).unwrap();
    assert_eq!(apply_amendment(&plan, &work, first).unwrap().version, 1);
    assert_policy(&plan, &work, "a", &["new", "two"]);
    assert!(plan_versions_dir(&work).join("1.applied").exists());
    assert_eq!(read_audit_rows(&work).unwrap().len(), 2);
}

#[test]
fn interrupted_audit_publication_keeps_previous_generation_readable() {
    let (_temp, work, plan) = fixture();
    let first = request("a", AmendmentPatch::Delete { index: 0 });
    apply_amendment(&plan, &work, first).unwrap();
    let mut second = request(
        "a",
        AmendmentPatch::Insert {
            index: 0,
            value: "new".into(),
        },
    );
    second.dispute_id = Some("second-a".into());
    transaction::fail_once(&work, "before-audit-publish");
    assert!(apply_amendment(&plan, &work, second.clone()).is_err());
    assert_eq!(count_amendments_for_stage(&work, "a").unwrap(), 1);
    assert_policy(&plan, &work, "a", &["two"]);
    apply_amendment(&plan, &work, second).unwrap();
    assert_eq!(count_amendments_for_stage(&work, "a").unwrap(), 2);
    assert_policy(&plan, &work, "a", &["new", "two"]);
}
