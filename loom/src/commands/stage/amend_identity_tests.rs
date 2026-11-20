//! `loom stage amend` also refuses a process that carries a session's identity
//! variables, so removing `LOOM_WORKTREE_PATH` does not bypass the guard.

use super::refuse_session_identity;

#[test]
fn a_session_identity_variable_refuses_the_amendment() {
    for (stage, session) in [
        (Some("some-stage"), None),
        (None, Some("session-1")),
        (Some("some-stage"), Some("session-1")),
    ] {
        let error = refuse_session_identity(stage, session)
            .expect_err("a process inside a session must not amend its own plan");
        let text = format!("{error:#}");
        assert!(text.contains("cannot amend"), "{text}");
        assert!(text.contains("LOOM_STAGE_ID"), "{text}");
    }
}

#[test]
fn absent_or_blank_identity_variables_name_no_session() {
    assert!(refuse_session_identity(None, None).is_ok());
    assert!(refuse_session_identity(Some(""), Some("  \t")).is_ok());
}
