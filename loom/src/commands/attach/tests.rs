//! Direct-attach SELECTION invariants only: `matches_for_stage` +
//! `pick_newest`. Discovery (`live_tmux_sessions`) and its fixtures moved to
//! `orchestrator::terminal::tmux::viewer`, along with its own tests.

use super::*;
use crate::orchestrator::terminal::tmux::viewer::tests::stub_session;
use chrono::Utc;

/// Exercised as the PAIR `attach_direct` itself calls, not through a
/// test-only wrapper: a wrapper would let the two halves drift apart from
/// the composition that actually ships.
#[test]
fn newest_for_stage_picks_the_newest_by_created_at() {
    let now = Utc::now();
    let oldest = stub_session("session-a", "stage-x", now - chrono::Duration::seconds(20));
    // b and c TIE on created_at; the pick must still be deterministic.
    let tied_first = stub_session("session-b", "stage-x", now - chrono::Duration::seconds(10));
    let tied_second = stub_session("session-c", "stage-x", now - chrono::Duration::seconds(10));
    // A NEWER session belonging to a DIFFERENT stage: the filter, not just
    // the pick, is what must exclude it.
    let other_stage = stub_session("session-d", "stage-y", now);
    let sessions = vec![oldest, tied_first, tied_second, other_stage];

    let matches = matches_for_stage(&sessions, "stage-x");
    let picked = pick_newest(&matches).expect("stage-x has live sessions");
    assert_eq!(
        picked.id, "session-c",
        "on a created_at tie, the pick must be deterministic"
    );
}

#[test]
fn newest_for_stage_returns_none_for_an_unknown_stage() {
    let sessions = vec![stub_session("session-a", "stage-x", Utc::now())];
    let matches = matches_for_stage(&sessions, "stage-unknown");
    assert!(pick_newest(&matches).is_none());
}

#[test]
fn no_live_sessions_message_names_the_work_dir_for_native_backend() {
    // A `loom attach` against the wrong repo of two must read differently
    // from a genuinely broken backend — both are otherwise "no live
    // sessions" and indistinguishable without the location.
    let work_dir = std::path::Path::new("/tmp/example-repo/.work");
    let message = no_live_sessions_message(work_dir, SessionBackendKind::Native);
    assert!(message.contains("/tmp/example-repo/.work"), "{message}");
    assert!(message.contains("tmux backend"), "{message}");
}

#[test]
fn no_live_sessions_message_names_the_work_dir_for_tmux_backend() {
    let work_dir = std::path::Path::new("/tmp/example-repo/.work");
    let message = no_live_sessions_message(work_dir, SessionBackendKind::Tmux);
    assert!(message.contains("/tmp/example-repo/.work"), "{message}");
    assert!(message.contains("No live tmux sessions"), "{message}");
}

#[test]
fn tmux_tmpdir_adoption_message_names_both_recorded_and_ambient_values() {
    let recorded = Some(std::ffi::OsString::from("/daemon/socket/dir"));
    let ambient = Some(std::ffi::OsString::from("/attach/shell/dir"));
    let message = format_tmux_tmpdir_adoption_message(&recorded, &ambient);
    assert!(message.contains("/daemon/socket/dir"), "{message}");
    assert!(message.contains("/attach/shell/dir"), "{message}");
}

#[test]
fn tmux_tmpdir_adoption_message_shows_unset_for_absent_values() {
    let message = format_tmux_tmpdir_adoption_message(&None, &None);
    assert_eq!(
        message,
        "Using the orchestrator's tmux socket dir (TMUX_TMPDIR=<unset>) instead of this shell's \
         (<unset>)"
    );
}
