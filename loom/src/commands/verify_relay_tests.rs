//! A session's `loom check` that fails still prints the `Host job: refused`
//! first line, whether the session cannot relay at all or a relay guard fails.

use tempfile::TempDir;

use super::{refuse, request_or_wait, LEGACY_REFUSAL};
use crate::relay::emit::RelayContext;

#[test]
fn a_legacy_session_is_refused_with_the_first_line() {
    let mut out = Vec::new();

    let code = refuse(&mut out, LEGACY_REFUSAL).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(
        text.contains("This session cannot relay verification (no LOOM_SCRATCH_DIR)."),
        "{text}"
    );
}

#[test]
fn a_failing_relay_guard_is_refused_with_the_first_line_and_writes_no_ticket() {
    let temp = TempDir::new().unwrap();
    let scratch = temp.path().join("session-1");
    let context = RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch.clone(),
        stage_id: Some("s1".to_string()),
        session_type: Some("stage".to_string()),
        worktree_path: None,
        work_dir: None,
    };
    let mut out = Vec::new();

    let code = request_or_wait(&context, "s1", temp.path(), &[], &mut out).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, 1, "{text}");
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(text.contains("relay scratch directory"), "{text}");
    assert!(!scratch.exists(), "no ticket was written");
}
