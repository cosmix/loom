//! Session I/O operations for continuation.
//!
//! The canonical session-persistence routines live in
//! [`crate::fs::session_files`]; this module re-exports them so existing
//! callers using the `crate::orchestrator::continuation::{save_session,
//! session_to_markdown}` path keep compiling. There is exactly one
//! implementation of each — see `fs/session_files.rs`.

pub use crate::fs::session_files::{save_session, session_to_markdown};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::Session;

    #[test]
    fn test_session_to_markdown() {
        let mut session = Session::new();
        session.id = "test-session-123".to_string();
        session.assign_to_stage("stage-1".to_string());

        let markdown = session_to_markdown(&session);

        assert!(markdown.contains("---"));
        assert!(markdown.contains("# Session: test-session-123"));
        assert!(markdown.contains("## Details"));
        assert!(markdown.contains("**Stage**: stage-1"));
    }
}
