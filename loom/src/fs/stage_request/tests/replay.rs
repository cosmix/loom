use super::*;
use crate::fs::jsonl_spool;
use crate::stage_service::QueuedRequest;

#[test]
fn truncate_failure_retries_without_refiling_a_dispute() {
    let (_temp, work_dir, root) = setup("replay", StageStatus::Executing, 2);
    append_to_spool(&root, &dispute_request(0)).unwrap();
    let file = std::fs::File::open(spool_path(&root)).unwrap();
    let mut refused = 0;
    let result = jsonl_spool::drain_file::<QueuedRequest>(file, &mut |entry| {
        apply_request(
            &work_dir,
            "replay",
            &entry.payload,
            &entry.identity(),
            &mut refused,
        )
    });
    assert!(result.is_err(), "a read-only descriptor cannot truncate");
    assert_eq!(refused, 0);
    assert_eq!(load_stage("replay", &work_dir).unwrap().dispute_count, 1);
    assert_eq!(read_pending(&root).unwrap().len(), 1);
    assert_eq!(
        drain_requests(&work_dir, "replay", &root).unwrap().applied,
        1
    );
    assert_eq!(load_stage("replay", &work_dir).unwrap().dispute_count, 1);
    only_dispute_on_disk(&work_dir, "replay");
    assert!(read_pending(&root).unwrap().is_empty());
}

#[test]
fn later_sink_failure_does_not_replay_the_first_dispute_effect() {
    let (_temp, work_dir, root) = setup("replay", StageStatus::Executing, 2);
    append_to_spool(&root, &dispute_request(0)).unwrap();
    append_to_spool(
        &root,
        &StageRequest::Block {
            reason: "later request".into(),
        },
    )
    .unwrap();
    let mut calls = 0;
    let mut refused = 0;
    let result = super::super::super::spool::drain_identified(&root, &mut |entry| {
        calls += 1;
        if calls == 2 {
            anyhow::bail!("injected second-sink failure");
        }
        apply_request(
            &work_dir,
            "replay",
            &entry.payload,
            &entry.identity(),
            &mut refused,
        )
    });
    assert!(result.is_err());
    assert_eq!(calls, 2);
    assert_eq!(load_stage("replay", &work_dir).unwrap().dispute_count, 1);
    assert_eq!(read_pending(&root).unwrap().len(), 2);
    drain_requests(&work_dir, "replay", &root).unwrap();
    assert_eq!(load_stage("replay", &work_dir).unwrap().dispute_count, 1);
    only_dispute_on_disk(&work_dir, "replay");
    assert!(read_pending(&root).unwrap().is_empty());
}

#[test]
fn freeze_stage_read_failure_keeps_the_request_pending() {
    let (_temp, work_dir, root) = setup("freeze", StageStatus::Executing, 0);
    let freeze = StageRequest::FreezeContracts {
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
    };
    append_to_spool(&root, &freeze).unwrap();
    std::fs::remove_dir_all(work_dir.join("stages")).unwrap();
    assert!(drain_requests(&work_dir, "freeze", &root).is_err());
    assert_eq!(read_pending(&root).unwrap().len(), 1);
}

#[test]
fn receipt_write_failure_does_not_publish_an_untracked_dispute() {
    let (_temp, work_dir, root) = setup("receipt", StageStatus::Executing, 1);
    append_to_spool(&root, &dispute_request(0)).unwrap();
    let entry: QueuedRequest =
        serde_json::from_str(std::fs::read_to_string(spool_path(&root)).unwrap().trim()).unwrap();
    let directory = work_dir.join("disputes/receipt");
    let obstacle = directory
        .join("requests")
        .join(format!("{}.json.tmp", entry.identity()));
    std::fs::create_dir_all(&obstacle).unwrap();
    assert!(drain_requests(&work_dir, "receipt", &root).is_err());
    assert!(!directory.join("1/request.md").exists());
    assert_eq!(read_pending(&root).unwrap().len(), 1);
    std::fs::remove_dir(&obstacle).unwrap();
    drain_requests(&work_dir, "receipt", &root).unwrap();
    assert_eq!(load_stage("receipt", &work_dir).unwrap().dispute_count, 1);
    assert!(directory.join("2/request.md").is_file());
    assert!(read_pending(&root).unwrap().is_empty());
}
