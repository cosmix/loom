use super::*;

fn accepted_verdict(work: &Path, plan: &Path) -> PathBuf {
    use crate::models::dispute::{
        CriterionField, DisputeKind, DisputeRequest, DisputeVerdict, DisputeVerdictRecord,
        PlanPatch,
    };
    prepare_disputed_stage(work, plan);
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
    let request = DisputeRequest {
        id: 1,
        stage_id: "a".into(),
        kind: DisputeKind::criterion(CriterionField::Acceptance, vec![0]),
        reason: "incorrect criterion".into(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    };
    fs::write(
        dir.join("request.md"),
        format!("---\n{}---\n", crate::yaml::to_string(&request).unwrap()),
    )
    .unwrap();
    fs::write(
        dir.join("verdict.md"),
        format!("---\n{}---\n", crate::yaml::to_string(&record).unwrap()),
    )
    .unwrap();
    dir.join("applied.marker")
}

fn prepare_disputed_stage(work: &Path, plan: &Path) {
    fs::write(work.join("config.toml"), format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\nbase_branch = \"main\"\n", plan.display(),
    )).unwrap();
    update_stage("a", work, |stage| {
        stage.status = crate::models::stage::StageStatus::NeedsAdjudication;
        stage.completed_commit = Some("old-policy-commit".into());
        Ok(())
    })
    .unwrap();
}

#[test]
fn adjudication_marker_waits_for_both_live_writes_on_retry() {
    use crate::orchestrator::adjudication::AdjudicatorRegistry;
    for phase in ["after-audit", "after-plan"] {
        let (_temp, work, plan) = fixture();
        let applied = accepted_verdict(&work, &plan);
        let registry = AdjudicatorRegistry::new();
        transaction::fail_once(&work, phase);
        let error = registry.apply_verdict(&work, "a", 1).unwrap_err();
        assert!(
            format!("{error:#}").contains(&format!("injected amendment failure at {phase}")),
            "{error:#}"
        );
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
