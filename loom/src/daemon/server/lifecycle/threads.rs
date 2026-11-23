use crate::tokens::TokenTracker;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// The daemon's background threads, joined at shutdown.
pub(super) struct ServerThreads {
    pub(super) orchestrator: Option<thread::JoinHandle<()>>,
    pub(super) log_tail: Option<thread::JoinHandle<()>>,
    pub(super) status_broadcast: thread::JoinHandle<()>,
    pub(super) quota_poller: thread::JoinHandle<()>,
    /// Shared with the scheduler, which joins it first when the run ends.
    pub(super) token_tracker: Arc<TokenTracker>,
}

impl ServerThreads {
    pub(super) fn join(self) {
        self.join_by(Instant::now() + JOIN_TIMEOUT);
    }

    fn join_by(self, deadline: Instant) {
        join_all(
            vec![
                (self.orchestrator, "orchestrator"),
                (self.log_tail, "log_tail"),
                (Some(self.status_broadcast), "status_broadcast"),
                (Some(self.quota_poller), "quota_poller"),
            ],
            deadline,
        );
        // Taken only after the orchestrator was waited for: a scheduler that
        // is ending the tracker takes it first and joins it itself.
        join_all(
            vec![(self.token_tracker.take_thread(), "token_tracker")],
            deadline,
        );
    }
}

pub(super) fn join_all(
    handles: Vec<(Option<thread::JoinHandle<()>>, &'static str)>,
    deadline: Instant,
) {
    for (handle, name) in handles {
        let Some(handle) = handle else { continue };
        while !handle.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            thread::sleep(remaining.min(Duration::from_millis(50)));
        }
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            eprintln!("Warning: {name} thread did not terminate within timeout");
        }
    }
}
