//! An integration-verify stage disputes a completed stage's wiring entry,
//! the entry its aggregated wiring re-verification failed.

use super::*;
use crate::models::dispute::CriterionTarget;
use crate::models::stage::StageType;

/// `stage-disp` as an integration-verify stage, plus `owner` in `status` with
/// `wiring` wiring checks.
fn setup_owner(status: StageStatus, wiring: usize) -> (TempDir, std::path::PathBuf) {
    let (tmp, work_dir) = setup(StageStatus::Executing, 1);
    update_stage("stage-disp", &work_dir, |stage| {
        stage.stage_type = StageType::IntegrationVerify;
        Ok(())
    })
    .unwrap();
    let mut owner = Stage {
        id: "owner".to_string(),
        name: "Owner".to_string(),
        status,
        merged: true,
        ..Stage::default()
    };
    for i in 0..wiring {
        owner.wiring.push(WiringCheck {
            source: "src/lib.rs".to_string(),
            pattern: format!("pub fn g{i}"),
            description: format!("g{i} is exported"),
            literal: true,
        });
    }
    save_stage(&owner, &work_dir).unwrap();
    (tmp, work_dir)
}

fn file_owned(work_dir: &Path, field: CriterionField, owner: &str, index: usize) -> Response {
    handle_dispute_criteria_identified(
        work_dir,
        "stage-disp",
        CriterionTarget {
            field,
            criterion_indices: vec![index],
            owner_stage: Some(owner.to_string()),
        },
        "stale upstream wiring".to_string(),
        None,
        None,
        None,
    )
    .unwrap()
}

#[test]
fn the_index_is_checked_against_the_owner_stages_wiring_list() {
    // The filing stage has no wiring at all; the owner has four checks.
    let (_tmp, work_dir) = setup_owner(StageStatus::Completed, 4);

    let message = refusal_message(file_owned(&work_dir, CriterionField::Wiring, "owner", 4));
    assert!(message.contains("out of range"), "msg: {message}");
    assert!(message.contains("stage 'owner' has 4"), "msg: {message}");

    let created = file_owned(&work_dir, CriterionField::Wiring, "owner", 3);
    assert!(
        matches!(created, Response::DisputeCreated { id: 1 }),
        "{created:?}"
    );
    let request =
        crate::orchestrator::adjudication::read_request(&work_dir, "stage-disp", 1).unwrap();
    assert_eq!(
        request.kind,
        DisputeKind::Criterion {
            criterion_indices: vec![3],
            field: CriterionField::Wiring,
            owner_stage: Some("owner".to_string()),
        }
    );
    assert_eq!(request.kind.amended_stage("stage-disp"), "owner");
}

#[test]
fn only_an_integration_verify_stage_disputes_another_stages_wiring() {
    let (_tmp, work_dir) = setup_owner(StageStatus::Completed, 4);
    update_stage("stage-disp", &work_dir, |stage| {
        stage.stage_type = StageType::Standard;
        Ok(())
    })
    .unwrap();
    let message = refusal_message(file_owned(&work_dir, CriterionField::Wiring, "owner", 0));
    assert!(
        message.contains("not an integration-verify stage"),
        "msg: {message}"
    );
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
}

#[test]
fn the_owner_must_be_completed_and_the_field_wiring() {
    let (_tmp, work_dir) = setup_owner(StageStatus::Executing, 4);
    let message = refusal_message(file_owned(&work_dir, CriterionField::Wiring, "owner", 0));
    assert!(message.contains("not completed"), "msg: {message}");

    let (_tmp, work_dir) = setup_owner(StageStatus::Completed, 4);
    let message = refusal_message(file_owned(
        &work_dir,
        CriterionField::Acceptance,
        "owner",
        0,
    ));
    assert!(message.contains("--field wiring"), "msg: {message}");
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
}

#[test]
fn an_unmerged_owner_is_refused() {
    let (_tmp, work_dir) = setup_owner(StageStatus::Completed, 4);
    update_stage("owner", &work_dir, |stage| {
        stage.merged = false;
        Ok(())
    })
    .unwrap();
    let message = refusal_message(file_owned(&work_dir, CriterionField::Wiring, "owner", 0));
    assert!(message.contains("not merged"), "msg: {message}");
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
}

#[test]
fn an_unknown_self_or_malformed_owner_is_refused() {
    let (_tmp, work_dir) = setup_owner(StageStatus::Completed, 4);
    for (owner, expected) in [
        ("missing", "--owner-stage 'missing'"),
        ("stage-disp", "names the filing stage"),
        ("../escape", "invalid --owner-stage"),
    ] {
        let message = refusal_message(file_owned(&work_dir, CriterionField::Wiring, owner, 0));
        assert!(message.contains(expected), "{owner}: {message}");
    }
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
}
