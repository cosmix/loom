//! Truth table for [`super::mode`].

use super::*;

fn env(session_id: Option<&str>, scratch_dir: Option<&str>) -> EnvSnapshot {
    EnvSnapshot {
        session_id: session_id.map(str::to_string),
        scratch_dir: scratch_dir.map(PathBuf::from),
        stage_id: Some("stage-a".to_string()),
        session_type: Some("stage".to_string()),
        worktree_path: None,
        work_dir: None,
        hook_context: false,
        control_broker: false,
    }
}

#[test]
fn operator_when_session_id_is_absent() {
    let snapshot = env(None, Some("/scratch"));
    assert!(matches!(mode(&snapshot), RelayMode::Operator));
}

#[test]
fn legacy_when_scratch_dir_is_absent() {
    let snapshot = env(Some("session-1"), None);
    assert!(matches!(mode(&snapshot), RelayMode::Legacy));
}

#[test]
fn relay_when_both_are_set_and_neither_override_is() {
    let snapshot = env(Some("session-1"), Some("/scratch/session-1"));
    let RelayMode::Relay(context) = mode(&snapshot) else {
        panic!("expected Relay mode");
    };
    assert_eq!(context.session_id, "session-1");
    assert_eq!(context.scratch_dir, PathBuf::from("/scratch/session-1"));
}

#[test]
fn hook_context_forces_operator_even_with_a_scratch_dir() {
    let mut snapshot = env(Some("session-1"), Some("/scratch/session-1"));
    snapshot.hook_context = true;
    assert!(matches!(mode(&snapshot), RelayMode::Operator));
}

#[test]
fn control_broker_forces_operator_even_with_a_scratch_dir() {
    let mut snapshot = env(Some("session-1"), Some("/scratch/session-1"));
    snapshot.control_broker = true;
    assert!(matches!(mode(&snapshot), RelayMode::Operator));
}

#[test]
fn hook_context_forces_operator_even_without_a_scratch_dir() {
    let mut snapshot = env(Some("session-1"), None);
    snapshot.hook_context = true;
    assert!(matches!(mode(&snapshot), RelayMode::Operator));
}
