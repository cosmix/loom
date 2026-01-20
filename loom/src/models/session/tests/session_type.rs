use super::super::*;

// =========================================================================
// SessionType tests
// =========================================================================

#[test]
fn test_session_type_default() {
    let session_type = SessionType::default();
    assert_eq!(session_type, SessionType::Stage);
}

#[test]
fn test_session_type_display() {
    assert_eq!(format!("{}", SessionType::Stage), "stage");
    assert_eq!(format!("{}", SessionType::Merge), "merge");
}
