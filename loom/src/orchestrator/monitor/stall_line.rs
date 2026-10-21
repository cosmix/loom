//! The silence at which a stage session's hung report stops being a warning
//! and becomes evidence its agent is gone, and the longer line a session gets
//! while its pinned `loom stage complete` runs.
//!
//! The completion is one foreground Bash call: no PostToolUse heartbeat fires
//! until every acceptance criterion and later check has run, and a stage with
//! many compiling criteria runs well past three response budgets. When the
//! PreToolUse branch of `loom-hooks/loom-control-complete.sh` admits the pinned
//! command it writes the session's heartbeat with [`COMPLETION_ACTIVITY`].
//! While that heartbeat is the session's latest, its line is
//! [`completion_line_secs`]. An observation or a subagent's heartbeat keeps
//! the activity; a main-agent progress heartbeat, the command's own
//! PostToolUse among them, replaces it and ends the allowance. The line stays
//! finite, so a completion that truly hangs is still recovered.

use crate::models::stage::Stage;
use crate::verify::DEFAULT_COMMAND_TIMEOUT;

use super::heartbeat::Heartbeat;

/// The multiple of a stage's response budget at which a still-silent session
/// stops being a warning and starts being evidence its agent is gone.
///
/// Three budgets, not two: the first report already costs the agent one full
/// budget, so a session that answers once more and then goes quiet again
/// restarts well below this line and cannot trip it.
const STALL_ESCALATION_MULTIPLIER: u64 = 3;

/// The heartbeat activity `loom-control-complete.sh` writes when it admits the
/// pinned completion command. The hook spells the same string.
pub(crate) const COMPLETION_ACTIVITY: &str = "Stage completion running";

/// Time past the stage's own commands for the completion's later work:
/// goal-backward and contract checks, impact tests and the review gate. Six
/// command timeouts; revisit if those checks gain timeouts of their own.
pub(crate) const COMPLETION_MARGIN_SECS: u64 = 1_800;

/// Whether a silence has run long enough to be treated as a dead session
/// rather than a slow one. `completion_line` is the session's
/// [`running_completion_line`], when it has one.
///
/// A stage may declare `subagent_timeout_secs: 0`, and nothing rejects it. A
/// zero budget calls every session hung on its first poll, so a stage that
/// declares no real budget gets warnings and nothing else.
pub(crate) fn is_stall_escalation(
    stale_duration_secs: u64,
    timeout_secs: u64,
    completion_line: Option<u64>,
) -> bool {
    timeout_secs > 0
        && stale_duration_secs
            >= timeout_secs
                .saturating_mul(STALL_ESCALATION_MULTIPLIER)
                .max(completion_line.unwrap_or(0))
}

/// The longest silence a running completion of `stage` is allowed: every
/// command it runs at the per-criterion timeout, plus the margin.
pub(crate) fn completion_line_secs(stage: &Stage) -> u64 {
    let commands = stage.acceptance.len()
        + stage.wiring_tests.len()
        + usize::from(stage.dead_code_check.is_some())
        + usize::from(stage.regression_test.is_some());
    u64::try_from(commands)
        .unwrap_or(u64::MAX)
        .saturating_mul(DEFAULT_COMMAND_TIMEOUT.as_secs())
        .saturating_add(COMPLETION_MARGIN_SECS)
}

/// The completion line of `session_id` on `stage`, when the session's own
/// latest heartbeat says its pinned completion is running. A heartbeat naming
/// another session is a previous attempt's and says nothing about this one.
pub(crate) fn running_completion_line(
    heartbeat: Option<&Heartbeat>,
    session_id: &str,
    stage: Option<&Stage>,
) -> Option<u64> {
    let running = heartbeat.is_some_and(|heartbeat| {
        heartbeat.session_id == session_id
            && heartbeat.activity.as_deref() == Some(COMPLETION_ACTIVITY)
    });
    stage.filter(|_| running).map(completion_line_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::AcceptanceCriterion;

    fn stage_with_criteria(count: usize) -> Stage {
        Stage {
            acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string()); count],
            ..Stage::default()
        }
    }

    fn completion_heartbeat(session_id: &str) -> Heartbeat {
        Heartbeat::new("stage-1".to_string(), session_id.to_string())
            .with_activity(COMPLETION_ACTIVITY.to_string())
    }

    /// The line is the stage's OWN budget times three, so a stage that
    /// declares a long one is judged against it and not against the default.
    #[test]
    fn escalation_scales_with_the_stages_own_budget() {
        assert!(is_stall_escalation(10_800, 3_600, None));
        assert!(!is_stall_escalation(10_799, 3_600, None));
    }

    /// A stage declaring a zero budget calls its session hung on the first
    /// poll. Warn about it, never kill on it, completion or not.
    #[test]
    fn a_zero_budget_never_escalates() {
        assert!(!is_stall_escalation(0, 0, None));
        assert!(!is_stall_escalation(86_400, 0, None));
        assert!(!is_stall_escalation(86_400, 0, Some(2_400)));
    }

    #[test]
    fn a_completion_line_moves_escalation_out_and_never_in() {
        assert!(!is_stall_escalation(2_399, 300, Some(2_400)));
        assert!(is_stall_escalation(2_400, 300, Some(2_400)));
        assert!(!is_stall_escalation(899, 300, Some(60)));
        assert!(is_stall_escalation(900, 300, Some(60)));
    }

    /// The incident's stage: 31 criteria ran past 45 minutes of silence.
    #[test]
    fn the_completion_line_counts_every_command_at_its_timeout() {
        assert_eq!(completion_line_secs(&stage_with_criteria(0)), 1_800);
        assert_eq!(
            completion_line_secs(&stage_with_criteria(31)),
            31 * 300 + 1_800
        );
    }

    #[test]
    fn only_this_sessions_completion_heartbeat_extends_the_line() {
        let stage = stage_with_criteria(2);
        let running = completion_heartbeat("session-1");
        assert_eq!(
            running_completion_line(Some(&running), "session-1", Some(&stage)),
            Some(2_400)
        );
        assert_eq!(
            running_completion_line(Some(&running), "session-2", Some(&stage)),
            None
        );
        assert_eq!(
            running_completion_line(Some(&running), "session-1", None),
            None
        );
        let tool = Heartbeat::new("stage-1".to_string(), "session-1".to_string())
            .with_activity("Tool executed: Bash".to_string());
        assert_eq!(
            running_completion_line(Some(&tool), "session-1", Some(&stage)),
            None
        );
    }
}
