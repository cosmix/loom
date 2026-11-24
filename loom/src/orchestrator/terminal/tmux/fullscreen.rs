//! Keeping the last frame of a fullscreen session.
//!
//! A pane running a fullscreen program (tmux's alternate screen) has no
//! scrollback: what it showed lives only on the visible screen, and the screen
//! is gone with the server at teardown. Polling can only keep what it
//! observed, so [`sample_fullscreen`] saves the visible screen of every live
//! session on its alternate screen, each poll replacing the file; teardown
//! (`history::save_history`) then keeps that last frame instead of the main
//! screen's scrollback.
//!
//! Sampling runs on the monitor's single poll thread, so it is best-effort and
//! bounded: one tmux operation per session, a time budget per poll, and a start
//! offset that rotates across polls so a server that never answers delays the
//! sessions after it for one poll, not for good.

use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::models::session::{Session, SessionStatus};

use super::capture::{PaneReads, PaneScreen, TmuxPane};
use super::history::save_last_screen;
use super::launch;

/// The most one poll spends starting new samples. One probe on a stuck server
/// takes up to `TMUX_PROBE_TIMEOUT`, so a poll lasts at most this plus one probe.
/// Revisit if monitor polls move off a single thread.
const SAMPLE_BUDGET: Duration = Duration::from_secs(2);

/// Where the next poll starts in the sorted live sessions, so the sessions after
/// a slow one are reached within a few polls. Only [`sample_fullscreen`] reads
/// it; the sampling itself takes the cursor as a parameter.
static NEXT_START: Mutex<usize> = Mutex::new(0);

/// Save the visible screen of each live tmux session now on its alternate
/// screen, through the real tmux reads.
pub(crate) fn sample_fullscreen(work_dir: &Path, sessions: &[Session]) {
    let mut start = NEXT_START.lock().unwrap_or_else(PoisonError::into_inner);
    *start = sample_fullscreen_budgeted(work_dir, sessions, &TmuxPane, SAMPLE_BUDGET, *start);
}

/// Whether the monitor samples `session`: a running session whose agent
/// process is alive. Liveness is process identity only, never tmux.
fn is_sampled(work_dir: &Path, session: &Session) -> bool {
    session.status == SessionStatus::Running && launch::pid_only_is_alive(work_dir, session)
}

/// [`sample_fullscreen`] through `pane`, from the first session, within the
/// default budget.
#[cfg(test)]
pub(super) fn sample_fullscreen_with(work_dir: &Path, sessions: &[Session], pane: &impl PaneReads) {
    sample_fullscreen_budgeted(work_dir, sessions, pane, SAMPLE_BUDGET, 0);
}

/// Sample the live sessions in id order, beginning at index `start` and wrapping,
/// and stop starting new samples once `budget` is spent. Returns the index the
/// next call should begin at: the first session this call did not reach.
pub(super) fn sample_fullscreen_budgeted(
    work_dir: &Path,
    sessions: &[Session],
    pane: &impl PaneReads,
    budget: Duration,
    start: usize,
) -> usize {
    let mut live: Vec<&Session> = sessions
        .iter()
        .filter(|session| is_sampled(work_dir, session))
        .collect();
    if live.is_empty() {
        return 0;
    }
    live.sort_by(|a, b| a.id.cmp(&b.id));
    let start = start % live.len();
    let began = Instant::now();
    let mut visited = 0;
    for session in live.iter().cycle().skip(start).take(live.len()) {
        if began.elapsed() >= budget {
            break;
        }
        if let Some(PaneScreen::Alternate(visible)) = pane.alternate_screen(session) {
            save_last_screen(work_dir, session, visible);
        }
        visited += 1;
    }
    (start + visited) % live.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::{load_session_exact, save_session};
    use crate::orchestrator::terminal::launch::write_test_pid_identity;
    use std::cell::RefCell;
    use tempfile::TempDir;

    /// A pane that answers alternate for every session and records who was read.
    struct Recording(RefCell<Vec<String>>);

    impl PaneReads for Recording {
        fn alternate_screen(&self, session: &Session) -> Option<PaneScreen> {
            self.0.borrow_mut().push(session.id.clone());
            Some(PaneScreen::Alternate(b"frame\n".to_vec()))
        }

        fn scrollback(&self, _session: &Session) -> Option<Vec<u8>> {
            None
        }
    }

    fn live_session(work: &Path, status: SessionStatus) -> Session {
        let mut session = Session::new();
        session.assign_to_stage("sampled".to_string());
        session.status = status;
        session.pid = Some(std::process::id());
        write_test_pid_identity(work, &session, std::process::id()).unwrap();
        save_session(&session, work).unwrap();
        session
    }

    #[test]
    fn only_running_sessions_are_sampled() {
        let work = TempDir::new().unwrap();
        let running = live_session(work.path(), SessionStatus::Running);
        let ended = live_session(work.path(), SessionStatus::Completed);
        let spawning = live_session(work.path(), SessionStatus::Spawning);
        let pane = Recording(RefCell::new(Vec::new()));

        sample_fullscreen_with(work.path(), &[ended, spawning, running.clone()], &pane);

        assert_eq!(*pane.0.borrow(), [running.id.as_str()]);
        assert!(
            load_session_exact(work.path(), &running.id)
                .unwrap()
                .unwrap()
                .fullscreen_seen
        );
    }

    /// A pane that sleeps on one session and records who was read, in order.
    struct Slow {
        slow_id: String,
        delay: Duration,
        reads: RefCell<Vec<String>>,
    }

    impl PaneReads for Slow {
        fn alternate_screen(&self, session: &Session) -> Option<PaneScreen> {
            self.reads.borrow_mut().push(session.id.clone());
            if session.id == self.slow_id {
                std::thread::sleep(self.delay);
            }
            Some(PaneScreen::Main)
        }

        fn scrollback(&self, _session: &Session) -> Option<Vec<u8>> {
            None
        }
    }

    /// Three live sessions in id order, and a pane slow on the first.
    fn three_with_a_slow_first(work: &Path) -> (Vec<Session>, Slow) {
        let mut sessions: Vec<Session> = (0..3)
            .map(|_| live_session(work, SessionStatus::Running))
            .collect();
        sessions.sort_by(|a, b| a.id.cmp(&b.id));
        let pane = Slow {
            slow_id: sessions[0].id.clone(),
            delay: Duration::from_millis(200),
            reads: RefCell::new(Vec::new()),
        };
        (sessions, pane)
    }

    #[test]
    fn a_spent_budget_leaves_the_remaining_sessions_unstarted() {
        let work = TempDir::new().unwrap();
        let (sessions, pane) = three_with_a_slow_first(work.path());

        let next =
            sample_fullscreen_budgeted(work.path(), &sessions, &pane, Duration::from_millis(50), 0);

        assert_eq!(*pane.reads.borrow(), [sessions[0].id.as_str()]);
        assert_eq!(next, 1, "the next call begins at the first session skipped");
    }

    #[test]
    fn the_advanced_cursor_reaches_the_sessions_behind_the_slow_one() {
        let work = TempDir::new().unwrap();
        let (sessions, pane) = three_with_a_slow_first(work.path());
        let budget = Duration::from_millis(50);

        let next = sample_fullscreen_budgeted(work.path(), &sessions, &pane, budget, 0);
        pane.reads.borrow_mut().clear();
        sample_fullscreen_budgeted(work.path(), &sessions, &pane, budget, next);

        let reads = pane.reads.borrow();
        assert_eq!(reads[..2], [sessions[1].id.clone(), sessions[2].id.clone()]);
    }

    #[test]
    fn a_budget_that_covers_every_session_visits_all_and_wraps_the_cursor() {
        let work = TempDir::new().unwrap();
        let (sessions, pane) = three_with_a_slow_first(work.path());

        let next = sample_fullscreen_budgeted(work.path(), &sessions, &pane, Duration::MAX, 2);

        let order: Vec<&str> = [2, 0, 1].iter().map(|i| sessions[*i].id.as_str()).collect();
        assert_eq!(*pane.reads.borrow(), order);
        assert_eq!(next, 2);
    }
}
