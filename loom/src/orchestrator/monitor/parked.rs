//! Telling a PARKED stage apart from a HUNG one.
//!
//! Both look identical from the heartbeat alone — a live PID that has stopped
//! writing `.loom/work/heartbeat/<stage-id>.json` — but they need opposite
//! responses from the operator:
//!
//! - **Hung**: the session is stuck partway through its work. Go look at it.
//! - **Parked**: the session finished, committed, and ended its turn without
//!   running `loom stage complete`. The work is done and gate-passing; the
//!   stage just needs the transition, and every dependent stage is blocked
//!   behind it until someone notices.
//!
//! Parking is not hypothetical and it is not rare. `hooks/commit-guard.sh` used
//! to refuse a stop that left a stage in `Executing` (`exit 2`), but was
//! downgraded to an advisory that returns 0, because Claude Code fires `Stop`
//! hooks during Task/subagent waits and blocking there killed sessions
//! mid-work. An advisory Stop hook's stderr is not fed back to the model, so
//! the reminder now reaches nobody: the agent ends its turn and the stage sits
//! in `Executing` indefinitely. Observed twice on one eight-stage plan
//! (2026-08-24 and 2026-08-25), the second time parked overnight for 12h45m
//! with every acceptance criterion already passing.
//!
//! This module does not change that policy — hung detection stays advisory,
//! nothing here kills, retries, or transitions anything. It only lets the
//! warning say which of the two situations the operator is looking at, and
//! name the command that resolves the recoverable one.
//!
//! # What "finished" means here
//!
//! Two observable facts, both required:
//!
//! 1. the stage branch carries at least one commit beyond its base, and
//! 2. the worktree has no uncommitted tracked changes.
//!
//! Untracked files are deliberately ignored: every loom worktree carries an
//! untracked `.claude/` and a `.loom/work` symlink, so a check that counted them
//! would never fire. [`has_uncommitted_changes`] already excludes them.
//!
//! Every probe is best-effort and every failure answers "no". A wrong `false`
//! costs the operator the sharper half of a warning they are getting anyway; a
//! wrong `true` would tell them to complete a stage that is genuinely stuck
//! partway through, which is how work gets lost.

use std::path::Path;

use crate::git::branch::{branch_name_for_stage, commits_ahead_of, has_uncommitted_changes};
use crate::models::session::Session;
use crate::models::stage::Stage;

/// Whether `stage` looks finished-but-not-completed rather than stuck.
///
/// Called only for a session already flagged hung, so the cost of two git
/// probes is paid once per hung report, not once per poll.
pub(super) fn stage_looks_finished(session: &Session, stage: &Stage) -> bool {
    let Some(worktree) = session.worktree_path.as_deref() else {
        // Knowledge stages run in the main repository with no worktree. The
        // branch/base reasoning below does not describe them, so decline.
        return false;
    };
    let Some(base) = stage.base_branch.as_deref() else {
        return false;
    };
    finished_in_worktree(worktree, &branch_name_for_stage(&stage.id), base)
}

/// The git half, split out so it can be tested against a real repository
/// without constructing a `Session` and a `Stage`.
fn finished_in_worktree(worktree: &Path, branch: &str, base: &str) -> bool {
    let Ok(ahead) = commits_ahead_of(branch, base, worktree) else {
        return false;
    };
    if ahead == 0 {
        return false;
    }
    // A clean tree is the second half: commits alone would also describe a
    // session that committed once and is still working on the rest.
    matches!(has_uncommitted_changes(worktree), Ok(false))
}

/// Render the operator-facing warning for a hung session.
///
/// Lives here rather than inline at the handler so the parked/hung distinction
/// is rendered where it is decided — and so it can be tested, which an inline
/// `eprintln!` never could be.
pub(crate) fn hung_warning(
    session_id: &str,
    stage_id: Option<&str>,
    stale_duration_secs: u64,
    timeout_secs: u64,
    last_activity: Option<&str>,
    finished_without_completing: bool,
) -> String {
    let stage_info = stage_id
        .map(|s| format!(" (stage '{s}')"))
        .unwrap_or_default();
    let activity_info = last_activity
        .map(|a| format!(", last: {a}"))
        .unwrap_or_default();

    let mut message = format!(
        "Warning: Session '{session_id}'{stage_info} appears hung \
(no heartbeat for {stale_duration_secs}s, budget {timeout_secs}s{activity_info})"
    );

    if finished_without_completing {
        let id = stage_id.unwrap_or("<stage-id>");
        message.push_str(&format!(
            "\n  It looks FINISHED rather than stuck: commits on its branch and a clean \
worktree. Run: loom stage complete {id}"
        ));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::Session;
    use crate::models::stage::Stage;
    use std::path::PathBuf;

    fn stage_with(base: Option<&str>) -> Stage {
        let mut stage = Stage::new("Build".to_string(), None);
        stage.id = "build".to_string();
        stage.base_branch = base.map(str::to_string);
        stage
    }

    fn session_with_worktree(path: Option<&str>) -> Session {
        let mut session = Session::new();
        if let Some(p) = path {
            session.set_worktree_path(PathBuf::from(p));
        }
        session
    }

    /// A knowledge stage runs in the main repo and has no worktree; the
    /// branch-and-base reasoning does not apply to it.
    #[test]
    fn a_session_without_a_worktree_is_never_called_finished() {
        let session = session_with_worktree(None);
        let stage = stage_with(Some("main"));
        assert!(!stage_looks_finished(&session, &stage));
    }

    #[test]
    fn a_stage_without_a_base_branch_is_never_called_finished() {
        let session = session_with_worktree(Some("/nonexistent/worktree"));
        let stage = stage_with(None);
        assert!(!stage_looks_finished(&session, &stage));
    }

    /// The probes run against a path that is not a git repository at all, so
    /// both fail. A failed probe must answer "no" rather than propagating or
    /// defaulting to "finished" — telling an operator to complete a stage that
    /// is actually stuck partway through is how work gets lost.
    #[test]
    fn a_failed_git_probe_answers_no() {
        assert!(!finished_in_worktree(
            Path::new("/nonexistent/worktree"),
            "loom/build",
            "main"
        ));
    }

    #[test]
    fn the_warning_names_the_completion_command_only_when_parked() {
        let parked = hung_warning("s1", Some("build"), 900, 300, Some("Bash"), true);
        assert!(parked.contains("loom stage complete build"));
        assert!(parked.contains("no heartbeat for 900s"));
        assert!(parked.contains("budget 300s"));
        assert!(parked.contains("last: Bash"));

        let stuck = hung_warning("s1", Some("build"), 900, 300, Some("Bash"), false);
        assert!(!stuck.contains("loom stage complete"));
        assert!(stuck.contains("appears hung"));
    }

    /// A hung session with no stage still renders; it must not print the word
    /// "None" or an empty command an operator could paste.
    #[test]
    fn a_missing_stage_id_degrades_without_printing_none() {
        let message = hung_warning("s1", None, 400, 300, None, true);
        assert!(!message.contains("None"));
        assert!(message.contains("<stage-id>"));
    }

    #[test]
    fn an_unresolvable_branch_answers_no() {
        let repo = std::env::temp_dir();
        assert!(!finished_in_worktree(
            &repo,
            "loom/definitely-not-a-branch",
            "definitely-not-a-base"
        ));
    }
}
