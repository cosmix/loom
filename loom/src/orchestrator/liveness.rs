//! Session liveness probe.
//!
//! Replaces the legacy `kill -0 <session.pid>` checks scattered across
//! the monitor with a single service that delegates to the native
//! backend's `is_session_alive`.

use anyhow::Result;
use std::sync::Arc;

use super::terminal::native::NativeBackend;
use crate::models::session::Session;

/// How a [`LivenessService`] resolves liveness for a given session.
#[derive(Clone)]
enum LivenessSource {
    /// Production: route via the native backend.
    Native(Arc<NativeBackend>),
    /// Test-only: every probe returns this fixed value.
    Fixed(bool),
}

/// Session liveness probe.
///
/// Holds an `Arc<NativeBackend>` so the monitor (which runs on its
/// own thread) and other callers can share a single backend instance.
#[derive(Clone)]
pub struct LivenessService {
    source: LivenessSource,
}

impl LivenessService {
    pub fn new(native: Arc<NativeBackend>) -> Self {
        Self {
            source: LivenessSource::Native(native),
        }
    }

    /// Test-only constructor that always reports the supplied liveness
    /// value. Lets monitor tests exercise the crash-detection path
    /// without spinning up a backend.
    pub fn fixed_for_tests(alive: bool) -> Self {
        Self {
            source: LivenessSource::Fixed(alive),
        }
    }

    /// Return `true` when the session's process is still running.
    /// Errors surfaced from the underlying backend bubble up
    /// (the monitor treats `Err` as "unknown" and skips crash reporting
    /// for that tick).
    pub fn is_alive(&self, session: &Session) -> Result<bool> {
        match &self.source {
            LivenessSource::Native(b) => b.is_session_alive(session),
            LivenessSource::Fixed(v) => Ok(*v),
        }
    }
}
