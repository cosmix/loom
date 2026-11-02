use super::*;

#[test]
fn a_prechecked_old_session_cannot_file_against_a_new_attempt() {
    let (_tmp, work_dir, _root) = setup("stage-disp", StageStatus::Executing, 1);
    crate::verify::transitions::update_stage("stage-disp", &work_dir, |stage| {
        stage.session = Some("session-new".into());
        Ok(())
    })
    .unwrap();
    let response = crate::stage_service::handle_dispute_criteria_for_session(
        &work_dir,
        "stage-disp",
        (crate::models::dispute::CriterionField::Acceptance, vec![0]),
        "stale".into(),
        None,
        None,
        (None, Some("session-old")),
    )
    .unwrap();
    assert!(matches!(response, Response::Error { .. }));
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
    let stage = crate::verify::transitions::load_stage("stage-disp", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.dispute_count, 0);
}
