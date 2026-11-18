//! Tests for read-only prerequisites.

use super::prereq::{collect_problems, ReadOnly};
use super::wire::server::{MissingPrereq, PrereqKind};
use crate::claude::auth::AuthProbe;

#[test]
fn read_only_reflects_and_preserves_problems() {
    let empty = ReadOnly::from_problems(vec![]);
    assert!(!empty.is_read_only());
    assert!(empty.problems().is_empty());

    let expected = MissingPrereq {
        what: PrereqKind::Tmux,
        message: "missing".into(),
        fix: "install".into(),
    };
    let state = ReadOnly::from_problems(vec![expected.clone()]);
    assert!(state.is_read_only());
    assert_eq!(state.problems(), &[expected]);
}

#[test]
fn reports_missing_tools_and_sandbox_in_order() {
    let problems = collect_problems(false, None, Some("sandbox problem".into()));
    assert_eq!(
        problems,
        vec![
            MissingPrereq {
                what: PrereqKind::Tmux,
                message: "tmux is not on PATH".into(),
                fix: "Install tmux".into()
            },
            MissingPrereq {
                what: PrereqKind::Claude,
                message: "claude is not on PATH".into(),
                fix: "Install Claude Code".into()
            },
            MissingPrereq {
                what: PrereqKind::Sandbox,
                message: "sandbox problem".into(),
                fix: "Install bubblewrap and socat".into()
            },
        ]
    );
}

#[test]
fn auth_outcomes_control_login_problem() {
    assert!(collect_problems(
        true,
        Some(AuthProbe::LoggedIn {
            method: "test".into()
        }),
        None
    )
    .is_empty());
    let logged_out = collect_problems(true, Some(AuthProbe::NotLoggedIn), None);
    assert_eq!(
        logged_out[0],
        MissingPrereq {
            what: PrereqKind::ClaudeLogin,
            message: "Claude Code is not logged in".into(),
            fix: "Run claude auth login".into(),
        }
    );
    let unknown = collect_problems(true, Some(AuthProbe::Unknown("timed out".into())), None);
    assert_eq!(
        unknown[0],
        MissingPrereq {
            what: PrereqKind::ClaudeLogin,
            message: "could not determine the Claude Code login (timed out)".into(),
            fix: "Run claude auth login".into(),
        }
    );
}
