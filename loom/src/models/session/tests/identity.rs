use super::super::*;
use crate::plan::schema::execution::BackendType;

// =========================================================================
// Runtime identity tests (Stage 1: backend, tracking_key, knowledge variant)
// =========================================================================

#[test]
fn tracking_key_matrix() {
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Stage),
        "loom-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Merge),
        "loom-merge-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::BaseConflict),
        "loom-base-conflict-auth"
    );
    assert_eq!(
        Session::derive_tracking_key("auth", SessionType::Knowledge),
        "loom-knowledge-auth"
    );
}

#[test]
fn assign_to_stage_populates_tracking_key() {
    let mut session = Session::new();
    assert!(session.tracking_key.is_empty());
    session.assign_to_stage("worker-pool".to_string());
    assert_eq!(session.tracking_key, "loom-worker-pool");
}

#[test]
fn merge_session_assign_uses_merge_prefix() {
    let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
    session.assign_to_stage("feature".to_string());
    assert_eq!(session.tracking_key, "loom-merge-feature");
}

#[test]
fn knowledge_constructor_derives_tracking_key() {
    let session = Session::new_knowledge("knowledge-bootstrap");
    assert_eq!(session.session_type, SessionType::Knowledge);
    assert_eq!(session.stage_id.as_deref(), Some("knowledge-bootstrap"));
    assert_eq!(session.tracking_key, "loom-knowledge-knowledge-bootstrap");
}

#[test]
fn backend_default_is_native_and_settable() {
    let mut session = Session::new();
    assert_eq!(session.backend, BackendType::Native);
    session.set_backend(BackendType::Container);
    assert_eq!(session.backend, BackendType::Container);
}

#[test]
fn container_identity_setter_populates_runtime_and_name() {
    let mut session = Session::new();
    session.set_container_identity("podman".to_string(), "loom-x".to_string());
    assert_eq!(session.runtime.as_deref(), Some("podman"));
    assert_eq!(session.container_name.as_deref(), Some("loom-x"));
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
        "context_limit": 200000,
        "created_at": "2024-01-01T00:00:00Z",
        "last_active": "2024-01-01T00:00:00Z"
    }"#;
    let session: Session = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(session.backend, BackendType::Native);
    assert!(session.runtime.is_none());
    assert!(session.container_name.is_none());
    assert!(session.tracking_key.is_empty());
    assert_eq!(session.session_type, SessionType::Stage);
}

#[test]
fn knowledge_session_serializes_with_session_type_knowledge() {
    let session = Session::new_knowledge("kb");
    let json = serde_json::to_string(&session).unwrap();
    assert!(json.contains("\"session_type\":\"knowledge\""));
    assert!(json.contains("\"tracking_key\":\"loom-knowledge-kb\""));
}

#[test]
fn clear_container_identity_resets_runtime_and_name() {
    let mut session = Session::new();
    session.set_container_identity("podman".to_string(), "loom-clear-test".to_string());
    assert!(session.runtime.is_some());
    assert!(session.container_name.is_some());

    session.clear_container_identity();
    assert!(
        session.runtime.is_none(),
        "runtime should be None after clear"
    );
    assert!(
        session.container_name.is_none(),
        "container_name should be None after clear"
    );
}
