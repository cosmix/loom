//! Bounded wait for the attach endpoint, and the honest per-session
//! diagnosis printed when the wait gives up.
//!
//! `loom run` and `loom attach` are normally run back to back. A stage
//! session's PID is recorded the moment its wrapper process appears, but the
//! session's own tmux server does not start accepting clients until a
//! moment later — so an operator who runs `loom attach` immediately hits
//! [`endpoint_ready`](crate::orchestrator::terminal::tmux::viewer::endpoint_ready)
//! returning `false` on the very first probe. `commands/attach`'s callers
//! used to treat that as a hard failure; this module instead polls for a
//! bounded window before giving up, and when it does give up, explains WHY
//! each live session could not be attached to instead of repeating one
//! blanket sentence for every possible cause — some of which, unlike a slow
//! spawn, can never resolve no matter how long the operator waits.

use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::models::session::Session;
use crate::orchestrator::terminal::tmux::viewer::{is_plain_identifier, tmux_session_name};
use crate::orchestrator::terminal::tmux::{socket_name, socket_path_for};

/// How long `loom attach` waits for a live session's tmux server to start
/// accepting clients before giving up. 30s comfortably covers the observed
/// spawn-to-`endpoint_ready` gap (low single-digit seconds) with headroom
/// for a loaded host, without leaving an operator staring at a hung command
/// for a session that is never coming back — see [`WaitOutcome::TimedOut`].
pub(super) const ENDPOINT_WAIT: Duration = Duration::from_secs(30);

/// How often the wait re-probes. 500ms keeps the probe count over a full
/// [`ENDPOINT_WAIT`] window small (60) while still catching a session that
/// becomes ready almost immediately without a noticeable pause.
pub(super) const ENDPOINT_POLL: Duration = Duration::from_millis(500);

/// The three ways [`poll_for_endpoint`] can end.
pub(super) enum WaitOutcome<T> {
    /// `probe` found something attachable; carries whatever it produced —
    /// the target session for direct attach, the pane list for the
    /// overview.
    Ready(T),
    /// The live session set this wait was watching went empty before
    /// `probe` ever succeeded. Reported separately from [`Self::TimedOut`]
    /// because "the session ended" and "the session never became ready" are
    /// different facts, and because it lets the wait stop the instant it
    /// happens instead of burning the rest of the deadline on a target that
    /// no longer exists.
    Ended,
    /// `deadline` elapsed with the live set still non-empty but `probe`
    /// never returning `Some`. Carries the sessions observed on the LAST
    /// poll, so the caller can build a per-session diagnosis
    /// ([`diagnose_sessions`]) without a further disk read.
    TimedOut(Vec<Session>),
}

/// Poll for something attachable, re-reading the live session set on every
/// attempt rather than trusting a snapshot taken once — a session can
/// appear (a slow spawn finishes) or disappear (a crash, or the operator
/// killing it) for the whole duration of this wait.
///
/// `live_sessions` and `probe` are both injected, exactly the way
/// [`attachable_panes`](crate::orchestrator::terminal::tmux::viewer::attachable_panes)
/// injects its `ready` predicate: the deadline/early-exit LOGIC below must
/// be testable without a tmux server, a socket directory, or real elapsed
/// time longer than a test can afford, and the tmux-specific probe
/// (`endpoint_ready`, a `has-session` call) belongs in production code, not
/// in this function. `on_wait_start` fires at most once, right before the
/// FIRST sleep — never when the first probe already succeeds, so a fast
/// attach prints nothing extra.
pub(super) fn poll_for_endpoint<T>(
    mut live_sessions: impl FnMut() -> Result<Vec<Session>>,
    mut probe: impl FnMut(&[Session]) -> Option<T>,
    deadline: Duration,
    interval: Duration,
    mut on_wait_start: impl FnMut(usize),
) -> Result<WaitOutcome<T>> {
    let started = Instant::now();
    let mut announced = false;
    loop {
        let sessions = live_sessions()?;
        if sessions.is_empty() {
            return Ok(WaitOutcome::Ended);
        }
        if let Some(found) = probe(&sessions) {
            return Ok(WaitOutcome::Ready(found));
        }
        // Checked BEFORE announcing/sleeping: a `deadline` of ZERO (the
        // non-TTY path — see `endpoint_wait_deadline`) must probe exactly
        // once and return, never printing a wait notice or sleeping at all.
        if started.elapsed() >= deadline {
            return Ok(WaitOutcome::TimedOut(sessions));
        }
        if !announced {
            on_wait_start(sessions.len());
            announced = true;
        }
        std::thread::sleep(interval);
    }
}

/// The deadline [`poll_for_endpoint`] should use: the full [`ENDPOINT_WAIT`]
/// window under a TTY, or zero otherwise.
///
/// A deadline of zero makes `poll_for_endpoint` probe exactly once and
/// return — see the comment on its timeout check — which is precisely
/// today's non-TTY behaviour. This module's whole invariant is that every
/// diagnostic is emitted BEFORE the TTY check (see `commands/attach/mod.rs`'s
/// module doc), so a harness with no terminal gets an immediate, testable
/// answer instead of a 30s hang.
pub(super) fn endpoint_wait_deadline() -> Duration {
    if std::io::stdout().is_terminal() {
        ENDPOINT_WAIT
    } else {
        Duration::ZERO
    }
}

/// Print the one-line progress notice `poll_for_endpoint` fires through
/// `on_wait_start`. A free function, not inlined at each call site, so
/// `attach_direct` and `run_overview` print identical wording.
pub(super) fn announce_wait(deadline: Duration, live_count: usize) {
    println!(
        "Waiting up to {}s for {live_count} live session(s) to start accepting clients...",
        deadline.as_secs()
    );
}

/// Which of the three ways an otherwise-live session can fail to become an
/// attach endpoint. Distinguished because exactly one of the three is
/// PERMANENT — see [`describe_failure`] — and telling an operator to
/// "re-run in a moment" for a condition that can never resolve is a lie.
enum EndpointFailure {
    /// The session's tmux socket file does not exist (yet, or any more).
    SocketMissing,
    /// The socket exists, but a `has-session` probe against it failed — the
    /// server is not accepting clients, or the tmux session name inside it
    /// does not match.
    ServerNotAccepting,
    /// The session's socket name or tmux session name contains a character
    /// `is_plain_identifier` rejects. Such a session is never offered a
    /// pane by `attachable_panes` in the first place (see that function's
    /// doc comment for why) — no amount of waiting renders it attachable.
    UnrenderableIdentifier,
}

/// Pure classification, injected exactly like
/// [`evaluate_new_session`](crate::orchestrator::terminal::tmux::evaluate_new_session):
/// whether a session's socket file exists on disk is the only fact that
/// needs a real filesystem, so it is the only thing passed in rather than
/// re-derived here. Checked first, ahead of `socket_exists`, because an
/// unrenderable identifier is disqualifying regardless of what the
/// filesystem says.
fn classify_endpoint_failure(
    socket: &str,
    tmux_session: &str,
    socket_exists: bool,
) -> EndpointFailure {
    if !is_plain_identifier(socket) || !is_plain_identifier(tmux_session) {
        EndpointFailure::UnrenderableIdentifier
    } else if !socket_exists {
        EndpointFailure::SocketMissing
    } else {
        EndpointFailure::ServerNotAccepting
    }
}

/// Human text for one [`EndpointFailure`]. The `UnrenderableIdentifier` arm
/// deliberately omits "re-run in a moment" — see the enum's doc comment —
/// while the other two, which really can resolve on their own, keep it.
fn describe_failure(failure: &EndpointFailure) -> &'static str {
    match failure {
        EndpointFailure::SocketMissing => {
            "its tmux socket does not exist yet — still spawning, or the server has already \
             ended; re-run `loom attach` in a moment"
        }
        EndpointFailure::ServerNotAccepting => {
            "its tmux socket exists but is not accepting clients yet — the server has not \
             finished starting, or has just ended; re-run `loom attach` in a moment"
        }
        EndpointFailure::UnrenderableIdentifier => {
            "its session id or tracking key cannot be rendered as a tmux identifier, so this \
             session can never be attached to — this will not resolve by waiting"
        }
    }
}

/// Classify one real session, reading its socket's presence off disk. The
/// only impure step in the diagnosis path — everything else here is the
/// pure `classify_endpoint_failure`/`describe_failure` pair above.
fn diagnose_session(session: &Session) -> EndpointFailure {
    let socket = socket_name(session);
    let tmux_session = tmux_session_name(session).unwrap_or_default();
    let socket_exists = socket_path_for(&socket).exists();
    classify_endpoint_failure(&socket, &tmux_session, socket_exists)
}

/// The report printed when [`poll_for_endpoint`] times out: names the
/// resolved work dir — so a `loom attach` run against the wrong repo of two
/// reads differently from a genuinely broken backend, see
/// `doc/loom/knowledge/mistakes/tmux-backend.md` — and, for every session
/// still live on the last poll, its stage id, session id, socket path, and
/// WHY it could not be attached to.
pub(super) fn diagnose_sessions(work_dir: &Path, sessions: &[Session]) -> String {
    let mut report = format!(
        "{} live session(s) in {} could not be attached to:",
        sessions.len(),
        work_dir.display()
    );
    for session in sessions {
        let socket_path = socket_path_for(&socket_name(session));
        report.push_str(&format!(
            "\n  - stage '{}' (session {}, socket {}): {}",
            session.stage_id.as_deref().unwrap_or("(none)"),
            session.id,
            socket_path.display(),
            describe_failure(&diagnose_session(session)),
        ));
    }
    report
}

#[cfg(test)]
#[path = "tests/wait.rs"]
mod tests;
