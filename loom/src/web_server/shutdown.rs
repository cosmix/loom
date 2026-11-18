//! Completion of a bounded stop whose admitted requests are still running.

use std::fmt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Stopping was requested, but owned server work has not finished.
///
/// Request and background-worker owners retain the listener and their resources,
/// including after this error is dropped. They may finish work admitted before
/// stopping. The address cannot be rebound until those owners finish; a caller
/// must not treat this outcome as completed shutdown or destroy their resources.
#[derive(Debug)]
pub struct ShutdownPending {
    local: SocketAddr,
    complete: Arc<AtomicBool>,
    failure: Option<Arc<Mutex<Option<String>>>>,
}

impl ShutdownPending {
    pub(crate) fn new(local: SocketAddr, complete: Arc<AtomicBool>) -> Self {
        Self {
            local,
            complete,
            failure: None,
        }
    }

    pub(crate) fn with_failure(mut self, failure: Arc<Mutex<Option<String>>>) -> Self {
        self.failure = Some(failure);
        self
    }

    /// A failure reported by the cleanup owner. Check after completion;
    /// absence while still pending does not establish successful cleanup.
    pub fn completion_error(&self) -> Option<String> {
        self.failure.as_ref().and_then(|failure| {
            failure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        })
    }

    /// Whether all tracked work owners and their listener have been released.
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    /// Wait at most `timeout` for ownership to be released. A false result
    /// leaves the same pending shutdown in force; it never cancels a commit.
    pub fn wait(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while !self.is_complete() {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return false;
            }
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
        true
    }
}

impl fmt::Display for ShutdownPending {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "server at {} is still stopping: outstanding work retains the listener and may finish updates",
            self.local
        )
    }
}

impl std::error::Error for ShutdownPending {}

/// Last field of the shared request owner: Rust drops earlier fields first,
/// so completion also means the listener and router have been released.
pub(super) struct Completion(pub(super) Arc<AtomicBool>);

impl Drop for Completion {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
