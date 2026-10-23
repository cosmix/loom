//! `loom stage waiting` on a contract writer's stage: an `AskUserQuestion`
//! waits as any session does, refused freeze or not.

use std::path::PathBuf;

use tempfile::TempDir;

use super::mark_waiting;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::StageStatus;
use crate::verify::contracts::refusal::{record_daemon_refusal, RefusalSource};
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::{create_stage, load_stage};

const STAGE: &str = "s1";

fn contract_writer_stage() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let mut writer = Session::new_contract(STAGE);
    writer.status = SessionStatus::Running;
    save_session(&writer, &work).unwrap();
    create_stage(&contract_stage(STAGE, &writer.id), &work).unwrap();
    (tmp, work, writer.id)
}

#[test]
fn a_refused_writer_waits_with_the_plain_reason() {
    let (_tmp, work, writer) = contract_writer_stage();
    let problems = vec!["src/lib.rs is not a contract file".to_string()];
    record_daemon_refusal(&work, STAGE, &writer, &problems, RefusalSource::Daemon).unwrap();

    assert_eq!(mark_waiting(&work, STAGE).unwrap(), None);

    let stage = load_stage(STAGE, &work).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForInput);
    assert_eq!(
        stage.review_reason, None,
        "the refusal is not the wait's reason"
    );
}

#[test]
fn a_frozen_writer_is_left_executing_for_its_handover() {
    let (_tmp, work, writer) = contract_writer_stage();
    write_test_freeze(&work, STAGE, &writer);

    let skipped = mark_waiting(&work, STAGE).unwrap().unwrap_or_default();

    assert!(skipped.contains("being handed over"), "{skipped}");
    assert_eq!(
        load_stage(STAGE, &work).unwrap().status,
        StageStatus::Executing
    );
}
