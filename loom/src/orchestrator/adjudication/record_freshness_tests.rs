use super::freeze_refusal::fixture;
use crate::fs::dispute_lock::lock_stage_dispute_dir;
use crate::models::dispute::{request_file, DisputeKind, DisputeRequest};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::record::record_verdict_text;
use crate::orchestrator::adjudication::verdict::{parse_and_validate_in, RecordContext};
use crate::verify::transitions::{save_stage, update_stage};

#[test]
fn freeze_accept_is_validated_against_stage_after_lifecycle_claim() {
    let fx = fixture();
    let mut stage = fx.stage.clone();
    stage.status = StageStatus::NeedsAdjudication;
    save_stage(&stage, &fx.work).unwrap();
    let kind = write_freeze_request(&fx.work);
    let raw = serde_json::json!({
        "verdict": "accept",
        "reasoning": "the format gate needs correction",
        "citations": [{"file": "src/a.rs", "line": 3, "excerpt": "x", "claim": "y"}],
        "plan_patch": {"patches": [{"field": "acceptance", "patch": {
            "op": "replace", "index": 1, "value": "cargo fmt --all -- --check"
        }}]}
    })
    .to_string();
    let context = RecordContext {
        work_dir: &fx.work,
        stage: &stage,
    };
    assert!(parse_and_validate_in(&raw, &kind, &context).is_ok());

    // The recorder blocks on this claim. A sibling amendment can change the
    // stage before it gets the claim; validation must see that new stage.
    let claim = lock_stage_dispute_dir(&fx.work.join("disputes/s1")).unwrap();
    let work = fx.work.clone();
    let verdict = raw.clone();
    let recorder = std::thread::spawn(move || record_verdict_text(&work, "s1", 1, &verdict, None));
    update_stage("s1", &fx.work, |current| {
        current.acceptance.truncate(1);
        Ok(())
    })
    .unwrap();
    drop(claim);

    let error = recorder.join().unwrap().unwrap_err();
    assert!(format!("{error:#}").contains("out of range"), "{error:#}");
    assert!(!fx.work.join("disputes/s1/1/verdict.md").exists());
}

fn write_freeze_request(work_dir: &std::path::Path) -> DisputeKind {
    let kind = DisputeKind::FreezeRefusal {
        problems: vec!["refused".into()],
    };
    let request = DisputeRequest {
        id: 1,
        stage_id: "s1".into(),
        kind: kind.clone(),
        reason: "refused".into(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: chrono::Utc::now(),
    };
    let request_path = request_file(&work_dir.join("disputes"), "s1", 1);
    std::fs::create_dir_all(request_path.parent().unwrap()).unwrap();
    std::fs::write(
        request_path,
        format!("---\n{}---\n", crate::yaml::to_string(&request).unwrap()),
    )
    .unwrap();
    kind
}
