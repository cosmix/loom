use super::*;

#[test]
fn loaded_inventory_drives_status_session_attribution() {
    let (_tmp, work) = super::tests::temp_work_dir();
    let mut session = Session::new();
    session.stage_id = Some("alpha".into());
    session.context_tokens = 1234;
    crate::fs::session_files::save_session(&session, work.root()).unwrap();
    let stage = Stage {
        id: "alpha".into(),
        session: Some(session.id.clone()),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    let sessions = load_all_sessions(&work).unwrap();
    let summary = build_stage_summary(&stage, &sessions, &work);
    assert_eq!(sessions.len(), 1);
    assert_eq!(summary.context_tokens, Some(1234));
    assert_eq!(summary.session_backend, Some(session.backend));
}
