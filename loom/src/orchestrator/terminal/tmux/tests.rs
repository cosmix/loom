//! Tests for tmux backend

use super::helpers::{check_tmux_available, parse_tmux_timestamp};
use super::types::TmuxSessionInfo;
use super::{BackendType, TerminalBackend, TmuxBackend};

#[test]
fn test_tmux_backend_session_name() {
    // Skip if tmux not available
    if check_tmux_available().is_err() {
        return;
    }

    let backend = TmuxBackend::new().unwrap();
    assert_eq!(backend.session_name("stage-1"), "loom-stage-1");
    assert_eq!(backend.backend_type(), BackendType::Tmux);
}

#[test]
fn test_parse_tmux_timestamp() {
    let dt = parse_tmux_timestamp("1704067200");
    assert!(dt.is_some());
    assert_eq!(dt.unwrap().timestamp(), 1704067200);

    assert!(parse_tmux_timestamp("invalid").is_none());
    assert!(parse_tmux_timestamp("").is_none());
}

#[test]
fn test_tmux_session_info() {
    use chrono::DateTime;

    let info = TmuxSessionInfo {
        name: "loom-stage-1".to_string(),
        created: DateTime::from_timestamp(1704067200, 0),
        attached: true,
        windows: 2,
    };

    assert_eq!(info.name, "loom-stage-1");
    assert!(info.created.is_some());
    assert!(info.attached);
    assert_eq!(info.windows, 2);
}
