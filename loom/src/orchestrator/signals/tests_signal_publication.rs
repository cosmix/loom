use super::*;
use crate::orchestrator::signals::generate::generate_signal_with_metrics;
use crate::verify::contracts::test_support::{contract, write_test_freeze};

#[test]
fn failed_signal_write_creates_no_delivery() {
    let temp = project_with_matching_knowledge();
    let work_dir = temp.path().join(".loom/work");
    let session = create_test_session();
    let stage = create_test_stage();
    assert!(retrieve_stage_pack(&work_dir, &stage).is_some());
    fs::create_dir_all(work_dir.join("signals").join(format!("{}.md", session.id))).unwrap();
    assert!(generate_signal(
        &session,
        &stage,
        &create_test_worktree(),
        &[],
        None,
        None,
        &work_dir
    )
    .is_err());
    assert!(load_deliveries(&work_dir, plan_key(&stage), &stage.id)
        .unwrap()
        .is_empty());
}

#[test]
fn failed_receipt_write_keeps_published_signal() {
    let temp = project_with_matching_knowledge();
    let work_dir = temp.path().join(".loom/work");
    let session = create_test_session();
    let stage = create_test_stage();
    assert!(retrieve_stage_pack(&work_dir, &stage).is_some());
    let dir = delivery_dir(&work_dir, plan_key(&stage), &stage.id);
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    fs::write(&dir, "block receipt directory").unwrap();
    let path = generate_signal(
        &session,
        &stage,
        &create_test_worktree(),
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();
    assert!(fs::read_to_string(path)
        .unwrap()
        .contains("## Knowledge Brief"));
}

#[test]
fn metrics_cover_the_same_complete_v2_signal() {
    let temp = project_with_matching_knowledge();
    let work_dir = temp.path().join(".loom/work");
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.plan_version = 2;
    stage.contracts = vec![contract()];
    stage.dispute_count = 1;
    write_test_freeze(&work_dir, &stage.id, &session.id);
    crate::orchestrator::adjudication::feedback::append_questions(
        &work_dir,
        &stage.id,
        &["Which test covers it?".to_string()],
    )
    .unwrap();
    let worktree = create_test_worktree();
    let path = generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();
    let ordinary = fs::read_to_string(path).unwrap();
    let (path, metrics) =
        generate_signal_with_metrics(&session, &stage, &worktree, &[], None, None, &work_dir)
            .unwrap();
    let measured = fs::read_to_string(path).unwrap();
    assert_eq!(ordinary, measured);
    assert!(measured.contains("## Frozen Contracts"));
    assert!(measured.contains("## Adjudicator Feedback"));
    assert_eq!(metrics.signal_size_bytes, measured.len());
    assert_eq!(
        metrics.stable_prefix_bytes
            + metrics.semi_stable_bytes
            + metrics.dynamic_bytes
            + metrics.recitation_bytes,
        measured.len()
    );
}
