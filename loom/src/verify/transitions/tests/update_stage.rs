use super::*;
use crate::models::stage::{Stage, StageStatus};

fn seed_stage(work_dir: &Path, id: &str) -> Stage {
    let stage = Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    create_stage(&stage, work_dir).unwrap();
    stage
}

#[test]
fn update_stage_applies_delta_and_returns_written_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    seed_stage(work_dir, "s1");

    let written = update_stage("s1", work_dir, |stage| {
        stage.dispute_count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(written.dispute_count, 1);
    assert_eq!(load_stage("s1", work_dir).unwrap().dispute_count, 1);
}

#[test]
fn update_stage_preserves_concurrent_field_written_after_load() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let stale = seed_stage(work_dir, "s2");
    update_stage("s2", work_dir, |stage| {
        stage.dispute_count = 5;
        Ok(())
    })
    .unwrap();

    assert_eq!(stale.dispute_count, 0);
    update_stage("s2", work_dir, |stage| {
        stage.retry_count += 1;
        Ok(())
    })
    .unwrap();

    let reloaded = load_stage("s2", work_dir).unwrap();
    assert_eq!(reloaded.dispute_count, 5);
    assert_eq!(reloaded.retry_count, 1);
}

#[test]
fn update_stage_leaves_file_untouched_on_closure_error() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    seed_stage(work_dir, "s3");
    update_stage("s3", work_dir, |stage| {
        stage.dispute_count = 9;
        Ok(())
    })
    .unwrap();

    let error = update_stage("s3", work_dir, |stage| {
        stage.dispute_count = 99;
        anyhow::bail!("closure failed")
    });
    assert!(error.is_err());
    assert_eq!(load_stage("s3", work_dir).unwrap().dispute_count, 9);
}

#[test]
fn update_stage_errors_when_file_missing() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    assert!(update_stage("does-not-exist", work_dir, |_| Ok(())).is_err());
}

#[test]
fn path_indexed_update_rejects_record_identity_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    seed_stage(work_dir, "canonical-id");
    let path = find_stage_file(&work_dir.join("stages"), "canonical-id")
        .unwrap()
        .unwrap();

    let error = update_stage_at_path("different-id", &path, work_dir, |stage| {
        stage.dispute_count = 99;
        Ok(())
    })
    .unwrap_err();
    assert!(error.to_string().contains("identity mismatch"));
    assert_eq!(
        load_stage("canonical-id", work_dir).unwrap().dispute_count,
        0
    );
}

#[test]
fn create_stage_refuses_to_replace_existing_record() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let mut original = seed_stage(work_dir, "create-only");
    original.close_reason = Some("retained".to_string());
    update_stage("create-only", work_dir, |stage| {
        stage.close_reason.clone_from(&original.close_reason);
        Ok(())
    })
    .unwrap();

    let replacement = Stage {
        id: "create-only".to_string(),
        name: "replacement".to_string(),
        ..Stage::default()
    };
    assert!(create_stage(&replacement, work_dir).is_err());

    let reloaded = load_stage("create-only", work_dir).unwrap();
    assert_eq!(reloaded.name, "Stage create-only");
    assert_eq!(reloaded.close_reason.as_deref(), Some("retained"));
}

#[test]
fn update_stage_concurrent_increments_have_no_lost_updates() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().to_path_buf();
    seed_stage(&work_dir, "s4");
    let barrier = Arc::new(Barrier::new(11));

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let work_dir = work_dir.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                update_stage("s4", &work_dir, |stage| {
                    stage.dispute_count += 1;
                    Ok(())
                })
                .unwrap();
            })
        })
        .collect();
    barrier.wait();
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(load_stage("s4", &work_dir).unwrap().dispute_count, 10);
}

#[test]
fn concurrent_field_updates_preserve_each_others_delta() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().to_path_buf();
    seed_stage(&work_dir, "s5");
    let barrier = Arc::new(Barrier::new(3));

    let first_dir = work_dir.clone();
    let first_barrier = Arc::clone(&barrier);
    let first = thread::spawn(move || {
        first_barrier.wait();
        update_stage("s5", &first_dir, |stage| {
            stage.dispute_count = 7;
            Ok(())
        })
        .unwrap();
    });

    let second_dir = work_dir.clone();
    let second_barrier = Arc::clone(&barrier);
    let second = thread::spawn(move || {
        second_barrier.wait();
        update_stage("s5", &second_dir, |stage| {
            stage.close_reason = Some("independent writer".to_string());
            Ok(())
        })
        .unwrap();
    });

    barrier.wait();
    first.join().unwrap();
    second.join().unwrap();

    let stage = load_stage("s5", &work_dir).unwrap();
    assert_eq!(stage.dispute_count, 7);
    assert_eq!(stage.close_reason.as_deref(), Some("independent writer"));
}
