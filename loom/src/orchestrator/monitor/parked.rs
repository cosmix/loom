//! Telling a PARKED stage apart from a HUNG one.
//!
//! Both look identical from the heartbeat alone — a live PID that has stopped
//! writing `.loom/work/heartbeat/<stage-id>.json` — but they need opposite
//! responses: a session may be stuck, or may have committed and ended its
//! turn without completing the stage. This module supplies a heuristic for
//! the warning; the evidence does not prove the work is complete or passes
//! its gates. Hung-session recovery is handled elsewhere.
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
//! would never fire.
//!
//! Every probe is best-effort and every failure answers "no". A wrong `false`
//! costs the operator the sharper half of a warning they are getting anyway; a
//! wrong `true` would tell them to complete a stage that is genuinely stuck
//! partway through, which is how work gets lost.

use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::worktree::WorktreeGit;
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
    match probe_finished(worktree, branch, base) {
        Ok(finished) => finished,
        Err(error) => {
            eprintln!(
                "Cannot inspect parked worktree {}: {error:#}",
                worktree.display()
            );
            false
        }
    }
}

fn probe_finished(worktree: &Path, branch: &str, base: &str) -> anyhow::Result<bool> {
    use anyhow::Context;
    let parent = worktree
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == ".worktrees"))
        .context("worktree is not under .worktrees")?;
    let root = parent.parent().context("worktree has no project root")?;
    let git = WorktreeGit::pinned(root, worktree)?;
    let range = format!("refs/heads/{base}..refs/heads/{branch}");
    let ahead = git.run_checked(&["rev-list", "--count", &range])?;
    if ahead.trim().parse::<usize>()? == 0 {
        return Ok(false);
    }
    // A clean tree is the second half: commits alone would also describe a
    // session that committed once and is still working on the rest.
    Ok(git
        .run_checked(&["status", "--porcelain", "--untracked-files=no"])?
        .is_empty())
}

/// Render the operator-facing warning for a hung session.
///
/// Lives here rather than inline at the handler so the parked/hung distinction
/// is rendered where it is decided — and so it can be tested, which an inline
/// `eprintln!` never could be. A session whose pinned completion is running
/// (`completion_line`, see [`super::stall_line`]) is neither: it is waiting on
/// its own acceptance run.
pub(crate) fn hung_warning(
    session_id: &str,
    stage_id: Option<&str>,
    stale_duration_secs: u64,
    timeout_secs: u64,
    last_activity: Option<&str>,
    finished_without_completing: bool,
    completion_line: Option<u64>,
) -> String {
    let stage_info = stage_id
        .map(|s| format!(" (stage '{s}')"))
        .unwrap_or_default();
    if let Some(line) = completion_line {
        return format!(
            "Warning: Session '{session_id}'{stage_info} is running its stage completion \
(no heartbeat for {stale_duration_secs}s; recovered only past {line}s of silence)"
        );
    }
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
        let parked = hung_warning("s1", Some("build"), 900, 300, Some("Bash"), true, None);
        assert!(parked.contains("loom stage complete build"));
        assert!(parked.contains("no heartbeat for 900s"));
        assert!(parked.contains("budget 300s"));
        assert!(parked.contains("last: Bash"));

        let stuck = hung_warning("s1", Some("build"), 900, 300, Some("Bash"), false, None);
        assert!(!stuck.contains("loom stage complete"));
        assert!(stuck.contains("appears hung"));
    }

    /// A running completion is reported as running, with the line it is
    /// allowed, and never told to run the completion it is already running.
    #[test]
    fn the_warning_says_a_completion_is_running() {
        let message = hung_warning("s1", Some("build"), 1_000, 300, None, true, Some(2_400));
        assert!(message.contains("is running its stage completion"));
        assert!(message.contains("recovered only past 2400s"));
        assert!(!message.contains("appears hung"));
        assert!(!message.contains("Run: loom stage complete"));
    }

    /// A hung session with no stage still renders; it must not print the word
    /// "None" or an empty command an operator could paste.
    #[test]
    fn a_missing_stage_id_degrades_without_printing_none() {
        let message = hung_warning("s1", None, 400, 300, None, true, None);
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

    #[test]
    fn parked_probe_pins_the_registered_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let worktree = crate::verify::contracts::test_support::contract_worktree(&repo, "s1");
        std::fs::write(worktree.join("README.md"), "committed\n").unwrap();
        for args in [
            ["add", "README.md"].as_slice(),
            ["commit", "-m", "work"].as_slice(),
        ] {
            let output = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(&worktree)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        assert!(finished_in_worktree(&worktree, "loom/s1", "main"));
        let marker = crate::verify::contracts::test_support::plant_foreign_git_dir(&worktree);
        assert!(!finished_in_worktree(&worktree, "loom/s1", "main"));
        assert!(!marker.exists());
        WorktreeGit::discovered(&worktree)
            .run(&["status", "--porcelain"])
            .unwrap();
        assert!(marker.exists());
    }
}
