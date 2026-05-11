//! Backend-aware session liveness probe.
//!
//! Replaces the legacy `kill -0 <session.pid>` checks scattered across
//! the monitor with a single service that delegates to the right
//! backend's `is_session_alive`.
//!
//! The previous design assumed every session had a host PID (`kill -0`
//! works against the host's process table). Container sessions don't —
//! their wrapper PID lives inside the container namespace. The monitor
//! used to read the host-side pid file and bypass the backend entirely,
//! which means a perfectly healthy container session would look "dead"
//! every poll cycle. `LivenessService::is_alive` routes through the
//! session's `backend` field so each runtime answers for its own
//! sessions (`docker inspect` for containers, `kill -0` for native).

use anyhow::Result;
use std::sync::Arc;

use super::terminal::dispatcher::BackendDispatcher;
use crate::models::session::Session;

/// How a [`LivenessService`] resolves liveness for a given session.
#[derive(Clone)]
enum LivenessSource {
    /// Production: route via the backend dispatcher.
    Dispatcher(Arc<BackendDispatcher>),
    /// Test-only: every probe returns this fixed value.
    Fixed(bool),
}

/// Backend-aware liveness probe.
///
/// Holds an `Arc<BackendDispatcher>` so the monitor (which runs on its
/// own thread) and other callers can share a single dispatcher instance.
#[derive(Clone)]
pub struct LivenessService {
    source: LivenessSource,
}

impl LivenessService {
    pub fn new(dispatcher: Arc<BackendDispatcher>) -> Self {
        Self {
            source: LivenessSource::Dispatcher(dispatcher),
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

    /// Return `true` when the session's process / container is still
    /// running. Errors surfaced from the underlying backend bubble up
    /// (the monitor treats `Err` as "unknown" and skips crash reporting
    /// for that tick).
    pub fn is_alive(&self, session: &Session) -> Result<bool> {
        match &self.source {
            LivenessSource::Dispatcher(d) => d.for_session(session).is_session_alive(session),
            LivenessSource::Fixed(v) => Ok(*v),
        }
    }
}
