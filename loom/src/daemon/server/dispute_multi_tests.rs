//! One criterion dispute names several entries of a list.

use super::*;

#[test]
fn one_filing_names_several_criteria_and_spends_one_dispute() {
    let (_tmp, work_dir) = setup(StageStatus::Executing, 15);
    let indices = vec![14, 13, 14];
    let resp = handle_dispute_criteria(
        &work_dir,
        "stage-disp",
        CriterionField::Acceptance,
        indices,
        "x".to_string(),
        None,
        None,
    )
    .unwrap();

    assert!(
        matches!(resp, Response::DisputeCreated { id: 1 }),
        "{resp:?}"
    );
    let content = std::fs::read_to_string(work_dir.join("disputes/stage-disp/1/request.md"));
    let yaml_chunk = content.as_deref().unwrap().split("---").nth(1).unwrap();
    let parsed: serde_json::Value = crate::yaml::from_str(yaml_chunk).unwrap();
    assert_eq!(parsed["criterion_indices"], serde_json::json!([13, 14]));
    assert!(!work_dir.join("disputes/stage-disp/2").exists());
    let stage = crate::verify::transitions::load_stage("stage-disp", &work_dir).unwrap();
    assert_eq!(stage.dispute_count, 1);
}

#[test]
fn one_index_out_of_range_refuses_the_whole_filing() {
    let (_tmp, work_dir) = setup(StageStatus::Executing, 15);
    let message = refusal_message(
        handle_dispute_criteria(
            &work_dir,
            "stage-disp",
            CriterionField::Acceptance,
            vec![13, 15],
            "x".to_string(),
            None,
            None,
        )
        .unwrap(),
    );

    assert!(
        message.contains("criterion index 15 out of range"),
        "msg: {message}"
    );
    assert!(!work_dir.join("disputes/stage-disp/1").exists());
    let stage = crate::verify::transitions::load_stage("stage-disp", &work_dir).unwrap();
    assert_eq!(stage.dispute_count, 0);
    assert_eq!(stage.status, StageStatus::Executing);
}
