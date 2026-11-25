//! A dispute of `--field artifacts` is range-checked against the stage's
//! `artifacts` list, not its acceptance list.

use super::*;

#[test]
fn artifacts_index_is_checked_against_the_artifacts_list() {
    // Five acceptance criteria but two artifact patterns: index 3 is valid
    // only for the acceptance list.
    let (_tmp, work_dir) = setup(StageStatus::Executing, 5);
    update_stage("stage-disp", &work_dir, |stage| {
        stage.artifacts = vec!["docs/a.md".to_string(), "docs/b.md".to_string()];
        Ok(())
    })
    .unwrap();
    let dispute = |index| dispute_field(&work_dir, CriterionField::Artifacts, index);

    let message = refusal_message(dispute(3));
    assert!(message.contains("out of range"), "msg: {message}");
    assert!(message.contains("--field artifacts"), "msg: {message}");
    assert!(!work_dir.join("disputes/stage-disp/1").exists());

    assert!(matches!(dispute(1), Response::DisputeCreated { id: 1 }));
    assert_eq!(filed_field(&work_dir), "artifacts");
}
