//! Reading a crashed session: what killed it, and whether trying again can
//! possibly help.
//!
//! Split out of `crash_handler`, which owns the orchestrator-side effects of a
//! crash (stage transition, retry announcement, graph sync). Everything here is
//! pure except for reading the session's captured stderr off disk, so the
//! decisions below are testable without a running daemon.

use std::path::{Path, PathBuf};

use crate::models::failure::FailureType;
use crate::models::session::Session;
use crate::orchestrator::retry::classify_failure;
use crate::orchestrator::spawner::read_log_tail;
use crate::orchestrator::terminal::native::{stderr_log_path, wrapper_script_path};

pub(super) const FAST_FAIL_WINDOW_SECS: i64 = 15;

/// Lines of stderr kept as stage evidence for a startup refusal. Short on
/// purpose: this text is read in `loom status` and by the attention model, and
/// a refusal says why in its last line or two. The crash report holds the
/// longer tail.
const STARTUP_REFUSAL_TAIL_LINES: usize = 20;

/// Whether a crash should be read as "`--remote-control` is unsupported here"
/// rather than as an ordinary stage failure.
///
/// # Why a verified PID, and not the backend
///
/// This was gated on `backend == Native`, to stop a tmux *hosting* failure
/// being misattributed to Remote Control. That reasoning does not survive
/// contact with the spawn path: every tmux hosting failure returns `Err` from
/// `TmuxBackend::spawn` and tears its PID file down, so it never produces a
/// tracked session that can later be reported as crashed. Both lanes reach
/// `Running` only after `await_session_pid` observes a real process. A
/// recorded PID is therefore the evidence that hosting succeeded — which is
/// what the backend check was reaching for — and it is available on both
/// lanes.
///
/// The gate's real effect was to deny the fallback to the tmux lane
/// entirely: a `--remote-control` that claude rejects exits at startup, the
/// retry re-spawns with identical flags, and the stage burns its whole
/// attempt budget on a flag that was never going to work — on the backend
/// loom uses when there is no GUI terminal to fall back to.
///
/// Latent, not observed. This was found while investigating a crash run that
/// turned out to have a different cause; no reproduction of the crash-loop
/// exists. It is fixed because the fallback provably cannot fire on the tmux
/// lane, not because it is known to have fired.
pub(super) fn is_remote_control_fast_fail(session_age_secs: i64, has_verified_pid: bool) -> bool {
    session_age_secs <= FAST_FAIL_WINDOW_SECS && has_verified_pid
}

/// A crash inside the fast-fail window, from a verified process, when the
/// remote-control fallback did NOT just fire, is a startup refusal: claude
/// exited before doing any work, and a retry with identical arguments will
/// exit the same way. Elapsed time is the evidence; nothing else is needed.
///
/// The remote-control exclusion is what separates the two readings of one
/// crash. When that fallback fires it has just changed what the next spawn
/// looks like — the retry omits `--remote-control` — so the retry is not
/// identical and is worth making. Every other fast crash is identical, and
/// spending the stage's whole attempt budget re-proving it is the failure this
/// predicate exists to stop.
///
/// # The trade-off, stated
///
/// Elapsed time cannot tell a refusal apart from a transient death that merely
/// lands in the same window — an OOM kill, disk contention, a flaky call during
/// Claude Code's own startup. Such a crash now blocks its stage with zero
/// retries instead of three. That is the deliberate side of the trade: a
/// refusal is deterministic and common (one bad settings file refuses every
/// session in the run), an early transient death is neither, and a stage
/// blocked with claude's own stderr as evidence is cheaper to fix than three
/// identical crashes with none. Widening the evidence — an exit code, a
/// recognised refusal string — would let both cases be served; nothing in the
/// spawn path records an exit code today.
pub(super) fn is_startup_refusal(
    session_age_secs: i64,
    has_verified_pid: bool,
    remote_control_fallback_applied: bool,
) -> bool {
    is_remote_control_fast_fail(session_age_secs, has_verified_pid)
        && !remote_control_fallback_applied
}

/// Name the refusal when the stderr tail says what it was; otherwise the
/// generic variant. [`FailureType::SandboxSetupFailure`] already means "the
/// security boundary could not be installed, never retry", which is exactly
/// what a `sandbox required but unavailable` exit is — reusing it keeps one
/// label on one condition instead of splitting the operator's attention
/// between two that mean the same thing.
fn classify_startup_refusal(stderr_tail: Option<&str>) -> FailureType {
    // Lowercase, because the tail is lowercased before the search.
    const SANDBOX_MARKERS: [&str; 2] = [
        "sandbox required but unavailable",
        "sandboxing requires wsl2",
    ];

    let Some(tail) = stderr_tail else {
        return FailureType::StartupRefusal;
    };
    let tail = tail.to_lowercase();
    if SANDBOX_MARKERS.iter().any(|marker| tail.contains(marker)) {
        FailureType::SandboxSetupFailure
    } else {
        FailureType::StartupRefusal
    }
}

/// The wrapper script that reproduces this session's exit when run by hand —
/// the shortest path from "loom says it crashed" to seeing claude's own error.
///
/// `None` for a session with no tracking key: the key is what names the script,
/// and a session that never reached a stage assignment has neither.
fn wrapper_reproduce_path(work_dir: &Path, session: &Session) -> Option<PathBuf> {
    if session.tracking_key.is_empty() {
        return None;
    }
    let pid_key = format!("{}-{}", session.tracking_key, session.id);
    Some(wrapper_script_path(work_dir, &pid_key))
}

/// The reason recorded on the stage for a refusal, naming both places an
/// operator goes next: the crash report holding claude's own words, and the
/// wrapper that reproduces the exit in one command.
fn startup_refusal_reason(
    work_dir: &Path,
    session: &Session,
    crash_report_path: Option<&Path>,
) -> String {
    let mut reason =
        format!("Claude exited within {FAST_FAIL_WINDOW_SECS}s of spawn before doing any work");
    if let Some(path) = crash_report_path {
        reason.push_str(&format!("; see crash report at {}", path.display()));
    }
    if let Some(wrapper) = wrapper_reproduce_path(work_dir, session) {
        reason.push_str(&format!(". Reproduce by hand: bash {}", wrapper.display()));
    }
    reason
}

/// How a crash was read: what gets recorded on the stage, plus the one-line
/// console note for a crash that will never be retried.
pub(super) struct CrashClassification {
    pub(super) failure_type: FailureType,
    pub(super) reason: String,
    pub(super) evidence: Vec<String>,
    /// Composed only for a startup refusal. Neither retry branch speaks for
    /// one — it is not auto-retryable, and it blocks on the first attempt
    /// rather than the last — so without this the operator would see a bare
    /// "session crashed" for a stage that has already stopped trying.
    pub(super) console_note: Option<String>,
}

/// The reading every crash gets unless claude refused to start.
///
/// Classify from a path-FREE reason. The crash-report path embeds
/// `path.display()` (under the user's repo); a repo path containing
/// "merge"/"token" would otherwise reclassify a crash as
/// MergeConflict/ContextExhausted (which `should_auto_retry` rejects),
/// permanently blocking auto-retry. See O-12.
pub(super) fn ordinary_crash(crash_report_path: Option<&Path>) -> CrashClassification {
    let classification_reason = "Session crashed";
    let reason = match crash_report_path {
        Some(path) => format!("Session crashed - see crash report at {}", path.display()),
        None => classification_reason.to_string(),
    };
    CrashClassification {
        failure_type: classify_failure(classification_reason),
        evidence: vec![reason.clone()],
        reason,
        console_note: None,
    }
}

/// The reading a fast crash from a verified process gets: claude's own stderr,
/// promoted out of a dead terminal pane into the stage's evidence, so
/// `loom status` and the attention model can say why the stage stopped.
pub(super) fn startup_refusal_crash(
    work_dir: &Path,
    session: &Session,
    crash_report_path: Option<&Path>,
) -> CrashClassification {
    let stderr_tail = read_log_tail(
        &stderr_log_path(work_dir, &session.id),
        STARTUP_REFUSAL_TAIL_LINES,
    );
    let stderr_lines: Vec<String> = stderr_tail
        .iter()
        .flat_map(|tail| tail.lines())
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();

    let last_words = stderr_lines
        .last()
        .map(String::as_str)
        .unwrap_or("no stderr captured");
    let mut console_note =
        format!("claude exited at startup and will not be retried. {last_words}.");
    if let Some(path) = crash_report_path {
        console_note.push_str(&format!(" Crash report: {}.", path.display()));
    }
    if let Some(wrapper) = wrapper_reproduce_path(work_dir, session) {
        console_note.push_str(&format!(" Reproduce: bash {}", wrapper.display()));
    }

    let reason = startup_refusal_reason(work_dir, session, crash_report_path);
    let mut evidence = vec![reason.clone()];
    evidence.extend(stderr_lines);
    CrashClassification {
        failure_type: classify_startup_refusal(stderr_tail.as_deref()),
        reason,
        evidence,
        console_note: Some(console_note),
    }
}

#[cfg(test)]
#[path = "crash_classification_tests.rs"]
mod tests;
