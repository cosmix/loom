//! The daemon's `tokens-tracker` thread with the flag that stops it, shared by
//! the server, which starts it and joins it at shutdown, and the scheduler,
//! which ends it before the run's final scan.

use super::tracker::spawn_token_tracker;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

/// A running tracker. Its stop flag is its own, so the scheduler can end it
/// while the daemon keeps serving.
pub struct TokenTracker {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl TokenTracker {
    /// Starts the tracker over the `.loom/work` directory `work_root`.
    pub fn spawn(work_root: PathBuf) -> Arc<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_token_tracker(work_root, Arc::clone(&stop));
        Arc::new(Self {
            stop,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// Asks the thread to stop: a pass under way stops reading files and
    /// still writes what it read.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// The thread, for the caller to join; `None` once it was taken.
    pub fn take_thread(&self) -> Option<JoinHandle<()>> {
        self.thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Stops the thread and waits until it has ended, so no pass of it runs
    /// after this returns. A tracker already taken returns at once.
    pub fn stop_and_join(&self) {
        self.stop();
        if let Some(thread) = self.take_thread() {
            // The thread catches every pass's panic; a join error leaves
            // nothing to clean up.
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "tracker_handle_tests.rs"]
mod tests;
