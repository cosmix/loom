//! Contracts: what teardown and the monitor save for a session whose pane ran
//! fullscreen (tmux's alternate screen), driven through the production writers
//! with scripted tmux answers, so no tmux server or socket is needed.
//!
//! The seam the implementation provides in `tmux/capture.rs`:
//!
//! ```text
//! pub(super) enum PaneScreen { Alternate(Vec<u8>), Main }
//!
//! pub(super) trait PaneReads {
//!     /// In one tmux server operation on the session's exact pane target:
//!     /// `Some(Alternate(visible bytes))` while the pane is still on its
//!     /// alternate screen, `Some(Main)` once it shows its main screen (any
//!     /// visible bytes discarded), `None` when the pane cannot be read.
//!     fn alternate_screen(&self, session: &Session) -> Option<PaneScreen>;
//!     /// The whole scrollback with colors, `None` when it cannot be read.
//!     fn scrollback(&self, session: &Session) -> Option<Vec<u8>>;
//! }
//! ```
//!
//! and the production entry points that take it, both best-effort and
//! returning nothing:
//!
//! ```text
//! history::save_history_with(work_dir: &Path, session: &Session, pane: &impl PaneReads)
//! fullscreen::sample_fullscreen_with(work_dir: &Path, sessions: &[Session], pane: &impl PaneReads)
//! ```
//!
//! `save_history` (called by `kill_session`) and `sample_fullscreen` (called by
//! `Monitor::poll`) pass the real tmux reads, which own the socket check, the
//! exact pane target and the bounded runner. Everything else (which sessions to
//! sample, the persisted `fullscreen_seen` flag, `capture_mode`, bounding and
//! the atomic write) happens in the entry points these contracts call.

use std::path::Path;

use tempfile::TempDir;

use super::capture::{PaneReads, PaneScreen};
use super::fullscreen::sample_fullscreen_with;
use super::history::{capture_mode, read_history, save_history_with, write_history, CaptureMode};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;

const MAIN_SCROLLBACK: &[u8] = b"MAIN-SCROLLBACK restored prompt\n";
const ALT_SAVED: &[u8] = b"ALT-SAVED last fullscreen frame\n";

/// What the scripted pane answers to `alternate_screen`, on every call.
#[derive(Clone, Copy)]
enum Answer {
    Alternate(&'static [u8]),
    Main,
    Unreadable,
}

/// A pane whose tmux answers are fixed; its scrollback is always
/// [`MAIN_SCROLLBACK`].
struct ScriptedPane(Answer);

impl PaneReads for ScriptedPane {
    fn alternate_screen(&self, _session: &Session) -> Option<PaneScreen> {
        match self.0 {
            Answer::Alternate(bytes) => Some(PaneScreen::Alternate(bytes.to_vec())),
            Answer::Main => Some(PaneScreen::Main),
            Answer::Unreadable => None,
        }
    }

    fn scrollback(&self, _session: &Session) -> Option<Vec<u8>> {
        Some(MAIN_SCROLLBACK.to_vec())
    }
}

/// A tmux session of a stage, persisted with `status` and `fullscreen`, whose
/// recorded process is this test process, so liveness checks answer alive.
fn tracked_session(work: &Path, status: SessionStatus, fullscreen: bool) -> Session {
    let mut session = Session::new();
    session.assign_to_stage("fullscreen-stage".to_string());
    session.backend = SessionBackendKind::Tmux;
    session.status = status;
    session.pid = Some(std::process::id());
    session.fullscreen_seen = fullscreen;
    write_test_pid_identity(work, &session, std::process::id()).unwrap();
    save_session(&session, work).unwrap();
    session
}

fn saved(work: &Path, session: &Session) -> Vec<u8> {
    read_history(work, &session.id)
        .unwrap()
        .unwrap_or_else(|| panic!("{} has no saved capture", session.id))
}

fn persisted(work: &Path, session: &Session) -> Session {
    load_session_exact(work, &session.id).unwrap().unwrap()
}

fn holds(bytes: &[u8], marker: &[u8]) -> bool {
    bytes.windows(marker.len()).any(|window| window == marker)
}

fn assert_capture_mode_table() {
    assert!(matches!(
        capture_mode(false, Some(true)),
        Some(CaptureMode::Visible)
    ));
    assert!(matches!(
        capture_mode(true, Some(true)),
        Some(CaptureMode::Visible)
    ));
    assert!(capture_mode(true, Some(false)).is_none());
    assert!(capture_mode(true, None).is_none());
    assert!(matches!(
        capture_mode(false, Some(false)),
        Some(CaptureMode::Full)
    ));
    assert!(matches!(capture_mode(false, None), Some(CaptureMode::Full)));
}

#[test]
fn a_fullscreen_session_keeps_its_last_screen_at_teardown() {
    let work = TempDir::new().unwrap();
    let dir = work.path();

    // The monitor saw the alternate screen: the flag and its capture are on
    // disk, while the caller still holds the record from before that.
    let seen = tracked_session(dir, SessionStatus::Running, true);
    write_history(dir, &seen.id, ALT_SAVED).unwrap();
    let mut stale = seen.clone();
    stale.fullscreen_seen = false;
    for answer in [Answer::Main, Answer::Unreadable, Answer::Alternate(b"")] {
        save_history_with(dir, &stale, &ScriptedPane(answer));
        assert_eq!(
            saved(dir, &seen),
            ALT_SAVED,
            "teardown replaced the last screen"
        );
        assert!(persisted(dir, &seen).fullscreen_seen);
    }

    // Teardown is the first to see the alternate screen, after a normal close
    // already marked the record terminal.
    let unseen = tracked_session(dir, SessionStatus::Completed, false);
    let mut caller = unseen.clone();
    caller.status = SessionStatus::Running;
    save_history_with(
        dir,
        &caller,
        &ScriptedPane(Answer::Alternate(b"ALT-AT-KILL\n")),
    );
    let bytes = saved(dir, &unseen);
    assert!(holds(&bytes, b"ALT-AT-KILL") && !holds(&bytes, b"MAIN-SCROLLBACK"));
    let record = persisted(dir, &unseen);
    assert!(
        record.fullscreen_seen,
        "first detection at kill was not recorded"
    );
    assert_eq!(record.status, SessionStatus::Completed);

    // An ordinary session still keeps its whole scrollback.
    let plain = tracked_session(dir, SessionStatus::Running, false);
    save_history_with(dir, &plain, &ScriptedPane(Answer::Main));
    assert!(holds(&saved(dir, &plain), b"MAIN-SCROLLBACK"));
    assert!(!persisted(dir, &plain).fullscreen_seen);

    assert_capture_mode_table();
}

#[test]
fn the_monitor_saves_each_alternate_frame_and_keeps_the_last() {
    let work = TempDir::new().unwrap();
    let dir = work.path();
    let live = tracked_session(dir, SessionStatus::Running, false);
    let sessions = [live.clone()];

    sample_fullscreen_with(
        dir,
        &sessions,
        &ScriptedPane(Answer::Alternate(b"ALT-FRAME-1\n")),
    );
    assert!(holds(&saved(dir, &live), b"ALT-FRAME-1"));
    assert!(
        persisted(dir, &live).fullscreen_seen,
        "detection was not recorded"
    );

    sample_fullscreen_with(
        dir,
        &sessions,
        &ScriptedPane(Answer::Alternate(b"ALT-FRAME-2\n")),
    );
    let bytes = saved(dir, &live);
    assert!(holds(&bytes, b"ALT-FRAME-2") && !holds(&bytes, b"ALT-FRAME-1"));

    for answer in [Answer::Main, Answer::Unreadable] {
        sample_fullscreen_with(dir, &sessions, &ScriptedPane(answer));
        let bytes = saved(dir, &live);
        assert!(
            holds(&bytes, b"ALT-FRAME-2"),
            "the last alternate frame was lost"
        );
        assert!(!holds(&bytes, b"MAIN-SCROLLBACK"));
        assert!(persisted(dir, &live).fullscreen_seen);
    }
}
