use super::super::*;

// =========================================================================
// Merge session tests
// =========================================================================

#[test]
fn test_new_merge_session() {
    let session = Session::new_merge("loom/feature".to_string(), "main".to_string());

    assert_eq!(session.session_type, SessionType::Merge);
    assert!(session.is_merge_session());
    assert_eq!(
        session.merge_source_branch,
        Some("loom/feature".to_string())
    );
    assert_eq!(session.merge_target_branch, Some("main".to_string()));
    assert_eq!(session.status, SessionStatus::Spawning);
}

#[test]
fn test_regular_session_is_not_merge() {
    let session = Session::new();

    assert_eq!(session.session_type, SessionType::Stage);
    assert!(!session.is_merge_session());
    assert!(session.merge_source_branch.is_none());
    assert!(session.merge_target_branch.is_none());
}

#[test]
fn test_merge_session_serialization() {
    let session = Session::new_merge("loom/stage-1".to_string(), "develop".to_string());

    // Test that serialization works
    let json = serde_json::to_string(&session).expect("Failed to serialize");
    assert!(json.contains("\"session_type\":\"merge\""));
    assert!(json.contains("\"merge_source_branch\":\"loom/stage-1\""));
    assert!(json.contains("\"merge_target_branch\":\"develop\""));

    // Test that deserialization works
    let deserialized: Session = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.session_type, SessionType::Merge);
    assert!(deserialized.is_merge_session());
    assert_eq!(
        deserialized.merge_source_branch,
        Some("loom/stage-1".to_string())
    );
    assert_eq!(
        deserialized.merge_target_branch,
        Some("develop".to_string())
    );
}

#[test]
fn test_regular_session_serialization_omits_merge_fields() {
    let session = Session::new();

    let json = serde_json::to_string(&session).expect("Failed to serialize");

    // merge_source_branch and merge_target_branch should be omitted (skip_serializing_if)
    assert!(!json.contains("merge_source_branch"));
    assert!(!json.contains("merge_target_branch"));
    // But session_type should still be present (defaults to stage)
    assert!(json.contains("\"session_type\":\"stage\""));
}

#[test]
fn test_deserialize_legacy_session_without_merge_fields() {
    // Simulate deserializing a session that was created before merge fields existed
    let legacy_json = r#"{
        "id": "session-abc123-1234567890",
        "stage_id": null,
        "worktree_path": null,
        "pid": null,
        "status": "spawning",
        "context_tokens": 0,
        "context_limit": 200000,
        "created_at": "2024-01-01T00:00:00Z",
        "last_active": "2024-01-01T00:00:00Z"
    }"#;

    let session: Session =
        serde_json::from_str(legacy_json).expect("Failed to deserialize legacy session");

    // Default values should be applied
    assert_eq!(session.session_type, SessionType::Stage);
    assert!(!session.is_merge_session());
    assert!(session.merge_source_branch.is_none());
    assert!(session.merge_target_branch.is_none());
}
