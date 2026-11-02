use super::super::*;

// =========================================================================
// Runtime identity tests (tracking_key, knowledge variant)
// =========================================================================

#[test]
fn tracking_key_matrix() {
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Stage),
        "loom-v2-stage-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Merge),
        "loom-v2-merge-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::BaseConflict),
        "loom-v2-base_conflict-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Knowledge),
        "loom-v2-knowledge-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Contract),
        "loom-v2-contract-auth"
    );
}

#[test]
fn contract_constructor_derives_tracking_key() {
    let session = Session::new_contract("parse-args");
    assert_eq!(session.session_type, SessionType::Contract);
    assert_eq!(session.stage_id.as_deref(), Some("parse-args"));
    assert_eq!(session.tracking_key, "loom-v2-contract-parse-args");
}

#[test]
fn contract_session_assign_keeps_contract_prefix() {
    let mut session = Session::new_contract("parse-args");
    session.assign_to_stage("parse-args".to_string());
    assert_eq!(session.tracking_key, "loom-v2-contract-parse-args");
}

#[test]
fn assign_to_stage_populates_tracking_key() {
    let mut session = Session::new();
    assert!(session.tracking_key.is_empty());
    session.assign_to_stage("worker-pool".to_string());
    assert_eq!(session.tracking_key, "loom-v2-stage-worker-pool");
}

#[test]
fn merge_session_assign_uses_merge_prefix() {
    let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
    session.assign_to_stage("feature".to_string());
    assert_eq!(session.tracking_key, "loom-v2-merge-feature");
}

#[test]
fn knowledge_constructor_derives_tracking_key() {
    let session = Session::new_knowledge("knowledge-bootstrap");
    assert_eq!(session.session_type, SessionType::Knowledge);
    assert_eq!(session.stage_id.as_deref(), Some("knowledge-bootstrap"));
    assert_eq!(
        session.tracking_key,
        "loom-v2-knowledge-knowledge-bootstrap"
    );
}

#[test]
fn legacy_session_without_new_fields_deserializes() {
    // Sessions written before runtime-identity fields existed must still parse.
    let legacy_json = r#"{
        "id": "session-abc-1",
        "stage_id": null,
        "worktree_path": null,
        "pid": null,
        "status": "spawning",
        "context_tokens": 0,
        "created_at": "2024-01-01T00:00:00Z",
        "last_active": "2024-01-01T00:00:00Z"
    }"#;
    let session: Session = serde_json::from_str(legacy_json).unwrap();
    assert!(session.tracking_key.is_empty());
    assert_eq!(session.session_type, SessionType::Stage);
}

#[test]
fn knowledge_session_serializes_with_session_type_knowledge() {
    let session = Session::new_knowledge("kb");
    let json = serde_json::to_string(&session).unwrap();
    assert!(json.contains("\"session_type\":\"knowledge\""));
    assert!(json.contains("\"tracking_key\":\"loom-v2-knowledge-kb\""));
}
