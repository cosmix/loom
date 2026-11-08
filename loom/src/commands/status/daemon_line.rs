//! The daemon headline of `loom status`: marker, message and hint, as plain data
//! a unit test can assert on, apart from the `colored`/println! plumbing.

use std::path::Path;

use chrono::Utc;
use colored::{ColoredString, Colorize};

use crate::daemon::DaemonStatus;
use crate::orchestrator::busy::{self, LoopState};
use crate::orchestrator::tick;
use crate::utils::format_elapsed;

/// Color category for a daemon-status line, decoupled from the `colored`
/// crate so [`daemon_status_line`]'s output is plain data a unit test can
/// assert on directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusTone {
    Ok,
    Warning,
    Error,
    Neutral,
}

pub(super) fn colorize(text: &str, tone: StatusTone) -> ColoredString {
    match tone {
        StatusTone::Ok => text.green(),
        StatusTone::Warning => text.yellow(),
        StatusTone::Error => text.red(),
        StatusTone::Neutral => text.dimmed(),
    }
}

/// Marker glyph, message, and trailing hint for the daemon-status line
/// printed by `execute_static`. A pure mapping from `(DaemonStatus,
/// LoopState)`, kept separate from the `colored`/println! plumbing so the
/// "never suggest `loom repair` for `Unreachable`" invariant is unit-testable
/// without parsing colored stdout.
pub(super) struct DaemonStatusLine {
    pub(super) marker: &'static str,
    pub(super) marker_tone: StatusTone,
    pub(super) message: String,
    pub(super) message_tone: StatusTone,
    pub(super) hint: &'static str,
}

const LIVE_HINT: &str = "loom status --live for real-time updates";
const SANDBOX_HINT: &str =
    "control socket unreachable from this sandbox (expected inside a stage worktree)";

/// What the tick says about the loop, for the daemons whose tick can be trusted.
///
/// Only `Running` and `Unreachable` read the tick: the flock proves a live daemon
/// owns the state directory in both cases. A `ProcessOnly` or `NotRunning` daemon
/// may have been killed mid-operation, leaving a busy tick behind, so it stays
/// `Unknown` and keeps its own headline.
pub(super) fn loop_state_for(work_dir: &Path, status: DaemonStatus) -> LoopState {
    if !matches!(status, DaemonStatus::Running | DaemonStatus::Unreachable) {
        return LoopState::Unknown;
    }
    busy::loop_state(tick::read(work_dir).ok().flatten().as_ref(), Utc::now())
}

/// Shared shape for both "orchestrator loop stalled" arms of
/// `daemon_status_line` (`Running` and `Unreachable`); only the message text
/// differs, based on whether the control socket happens to be reachable
/// from this sandbox.
fn stalled_status_line(message: &str) -> DaemonStatusLine {
    DaemonStatusLine {
        marker: "●",
        marker_tone: StatusTone::Error,
        message: message.to_string(),
        message_tone: StatusTone::Error,
        hint: "see below",
    }
}

/// The headline for a wrapped operation that is running within its limits.
fn busy_status_line(
    what: &str,
    elapsed_secs: i64,
    over_ceiling: bool,
    hint: &'static str,
) -> DaemonStatusLine {
    let tone = if over_ceiling {
        StatusTone::Warning
    } else {
        StatusTone::Ok
    };
    DaemonStatusLine {
        marker: "●",
        marker_tone: tone,
        message: format!(
            "daemon running, busy {what} ({})",
            format_elapsed(elapsed_secs)
        ),
        message_tone: tone,
        hint,
    }
}

/// Build the [`DaemonStatusLine`] for `status`, accounting for the loop `state`.
///
/// `Running` and `Unreachable` get a stalled or busy line ahead of their
/// plain-status arm: the stall is the more urgent fact, and a busy operation
/// explains why the loop is quiet. Without this, the plain arms would print a
/// green "daemon running" headline while `print_scheduler_alerts` lists a
/// stalled-loop alert right underneath it. The `Unreachable` stalled message
/// keeps the sandbox note attached, the busy one keeps it in the hint. Still no
/// repair hint: the daemon itself may be fine even though its loop is stalled.
///
/// `Unreachable` (otherwise): A-16 / sandboxed connect denial. The singleton
/// flock already proved a live daemon owns this state directory (see
/// `DaemonServer::check_status`'s `LockState::Held` arm) — the failed
/// `connect()` is a property of THIS process's sandbox, not the daemon. It
/// renders like a healthy `Running` daemon, with a note instead of
/// `ProcessOnly`'s repair hint: a sandboxed `loom status` must never claim
/// the socket is "missing" or suggest repairing (and via `loom repair`,
/// restarting) a daemon that is actually fine.
pub(super) fn daemon_status_line(status: DaemonStatus, state: &LoopState) -> DaemonStatusLine {
    let hint = match status {
        DaemonStatus::Unreachable => SANDBOX_HINT,
        _ => LIVE_HINT,
    };
    match (status, state) {
        (DaemonStatus::Running, LoopState::Stalled { .. }) => {
            stalled_status_line("daemon running, orchestrator loop stalled")
        }
        (DaemonStatus::Unreachable, LoopState::Stalled { .. }) => stalled_status_line(
            "daemon running, orchestrator loop stalled (socket unreachable from this sandbox)",
        ),
        (
            DaemonStatus::Running | DaemonStatus::Unreachable,
            LoopState::Busy {
                busy,
                elapsed_secs,
                over_ceiling,
            },
        ) => busy_status_line(
            &busy.op.describe(&busy.stage_id),
            *elapsed_secs,
            *over_ceiling,
            hint,
        ),
        (DaemonStatus::Running | DaemonStatus::Unreachable, _) => DaemonStatusLine {
            marker: "●",
            marker_tone: StatusTone::Ok,
            message: "daemon running".to_string(),
            message_tone: StatusTone::Neutral,
            hint,
        },
        (DaemonStatus::ProcessOnly, _) => DaemonStatusLine {
            marker: "●",
            marker_tone: StatusTone::Warning,
            message: "daemon process alive, socket missing".to_string(),
            message_tone: StatusTone::Warning,
            hint: "try `loom repair`",
        },
        (DaemonStatus::NotRunning, _) => DaemonStatusLine {
            marker: "○",
            marker_tone: StatusTone::Neutral,
            message: "daemon stopped".to_string(),
            message_tone: StatusTone::Neutral,
            hint: "run `loom run` to start",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::busy::{Busy, BusyOp};

    const TICKING: LoopState = LoopState::Ticking { age_secs: 2 };

    fn stalled() -> LoopState {
        LoopState::Stalled {
            age_secs: 90,
            busy: None,
            elapsed_secs: None,
        }
    }

    fn busy(elapsed_secs: i64, over_ceiling: bool) -> LoopState {
        LoopState::Busy {
            busy: Busy {
                op: BusyOp::BriefGeneration,
                stage_id: "stage-a".to_string(),
                started_at: Utc::now(),
            },
            elapsed_secs,
            over_ceiling,
        }
    }

    #[test]
    fn unreachable_status_never_suggests_repair() {
        let line = daemon_status_line(DaemonStatus::Unreachable, &TICKING);
        assert!(
            !line.hint.contains("loom repair"),
            "a sandboxed connect failure must never suggest repairing a healthy daemon"
        );
        assert!(
            !line.message.contains("missing"),
            "Unreachable must not claim the socket is missing — it's a caller-side sandbox limit"
        );
        assert_eq!(line.message, "daemon running");
        assert_eq!(line.marker_tone, StatusTone::Ok);
    }

    #[test]
    fn unreachable_status_with_stalled_loop_reports_the_stall() {
        let line = daemon_status_line(DaemonStatus::Unreachable, &stalled());
        assert_eq!(
            line.marker_tone,
            StatusTone::Error,
            "a stalled loop must not render as healthy just because this caller is sandboxed"
        );
        assert!(
            line.message.contains("stalled"),
            "the headline must report the stall, not just \"daemon running\": {}",
            line.message
        );
        assert!(
            !line.hint.contains("loom repair"),
            "the daemon may be fine even with a stalled loop — still no repair hint"
        );
    }

    #[test]
    fn process_only_status_still_suggests_repair() {
        let line = daemon_status_line(DaemonStatus::ProcessOnly, &TICKING);
        assert!(line.hint.contains("loom repair"));
        assert_eq!(line.marker_tone, StatusTone::Warning);
    }

    #[test]
    fn stalled_running_status_overrides_healthy_running() {
        let line = daemon_status_line(DaemonStatus::Running, &stalled());
        assert_eq!(line.marker_tone, StatusTone::Error);
        assert!(line.message.contains("stalled"));
    }

    #[test]
    fn not_running_status_uses_hollow_marker() {
        let line = daemon_status_line(DaemonStatus::NotRunning, &LoopState::Unknown);
        assert_eq!(line.marker, "○");
    }

    #[test]
    fn busy_under_the_ceiling_is_ok_toned() {
        let line = daemon_status_line(DaemonStatus::Running, &busy(90, false));
        assert_eq!(
            line.message,
            "daemon running, busy generating the brief for stage-a (1m30s)"
        );
        assert_eq!(line.marker_tone, StatusTone::Ok);
        assert_eq!(line.message_tone, StatusTone::Ok);
        assert_eq!(line.hint, LIVE_HINT);
    }

    #[test]
    fn busy_over_the_ceiling_is_warning_toned() {
        let line = daemon_status_line(DaemonStatus::Running, &busy(400, true));
        assert!(line.message.ends_with("(6m40s)"), "got {}", line.message);
        assert_eq!(line.marker_tone, StatusTone::Warning);
        assert_eq!(line.message_tone, StatusTone::Warning);
    }

    #[test]
    fn unreachable_busy_keeps_the_sandbox_note_in_the_hint() {
        let line = daemon_status_line(DaemonStatus::Unreachable, &busy(90, false));
        assert!(line.message.starts_with("daemon running, busy "));
        assert_eq!(line.hint, SANDBOX_HINT);
    }

    #[test]
    fn stopped_daemon_ignores_a_busy_state() {
        let line = daemon_status_line(DaemonStatus::NotRunning, &busy(90, false));
        assert_eq!(line.message, "daemon stopped");
        let line = daemon_status_line(DaemonStatus::ProcessOnly, &busy(90, true));
        assert!(line.message.contains("socket missing"));
    }

    #[test]
    fn only_the_loop_state_makes_the_headline_stalled() {
        // A Critical alert of another kind leaves the loop state Ticking.
        let line = daemon_status_line(DaemonStatus::Running, &TICKING);
        assert_eq!(line.message, "daemon running");
        assert_eq!(line.message_tone, StatusTone::Neutral);
    }

    #[test]
    fn loop_state_is_unknown_unless_the_daemon_is_alive() {
        let temp = tempfile::TempDir::new().unwrap();
        tick::record(temp.path(), tick::Phase::Events);
        assert!(matches!(
            loop_state_for(temp.path(), DaemonStatus::Running),
            LoopState::Ticking { .. }
        ));
        assert!(matches!(
            loop_state_for(temp.path(), DaemonStatus::Unreachable),
            LoopState::Ticking { .. }
        ));
        for status in [DaemonStatus::ProcessOnly, DaemonStatus::NotRunning] {
            assert_eq!(loop_state_for(temp.path(), status), LoopState::Unknown);
        }
    }
}
