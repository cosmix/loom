use super::threads_with_state_root;
use crate::codex_lifecycle::test_support::{Fixture, INV_A, STAGE};
use serde_json::json;
use std::fs;

#[test]
fn live_and_completed_jobs_need_exact_authorized_session_and_trusted_state_root() {
    let fixture = Fixture::new().unwrap();
    let row = fixture.authorization_value("unit", INV_A);
    fixture.write_authorizations(&[row.clone()]).unwrap();
    let authorization = fixture.authorization("unit", INV_A).unwrap();
    let job = fixture.write_job(&authorization, "job", "running").unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&job).unwrap()).unwrap();
    value["threadId"] = json!("live-thread");
    fs::write(&job, serde_json::to_vec(&value).unwrap()).unwrap();
    let sessions = [fixture.session.clone()];
    let read = || threads_with_state_root(&fixture.work_dir, &sessions, &fixture.state_root);
    assert_eq!(read(), vec![(STAGE.into(), "live-thread".into())]);
    fixture
        .write_job(&authorization, "job", "completed")
        .unwrap();
    assert_eq!(read(), vec![(STAGE.into(), "thread-1".into())]);
    assert!(threads_with_state_root(&fixture.work_dir, &[], &fixture.state_root).is_empty());
    assert!(threads_with_state_root(&fixture.work_dir, &sessions, &fixture.workspace).is_empty());
    value["sessionId"] = json!("unrelated-session");
    fs::write(&job, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(read().is_empty());
    fixture
        .write_job(&authorization, "job", "completed")
        .unwrap();
    fixture.write_authorizations(&[row.clone(), row]).unwrap();
    assert!(
        read().is_empty(),
        "ambiguous duplicate authorizations are rejected"
    );
}
