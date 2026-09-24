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
    assert_eq!(format!("{}", SessionType::Contract), "contract");
}

#[test]
fn contract_session_type_serializes_as_its_display_spelling() {
    let json = serde_json::to_string(&SessionType::Contract).unwrap();
    assert_eq!(json, "\"contract\"");
    let parsed: SessionType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, SessionType::Contract);
}
