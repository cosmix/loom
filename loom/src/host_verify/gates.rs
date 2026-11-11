//! The daemon's completion gates (`daemon::check_completion_gates`: test
//! integrity and the recorded review) as the refusal every completion path
//! gives. They read the stage's state files and its worktree's change
//! fingerprint, so they take seconds where a completion run takes minutes:
//! the daemon runs them before it queues a run (`inbox_drain::complete`, the
//! verdict sweep) and again when it applies one (`apply`, `verdict_complete`),
//! since the tree or the review can change while the run goes.

use std::path::Path;

/// Why the completion gates refuse `stage_id`, or `None` when they pass. The
/// gates report a verdict and a git or I/O failure as the same untyped error,
/// so any gate error is a refusal; the next completion request runs them
/// again.
pub(crate) fn refusal(work_dir: &Path, stage_id: &str) -> Option<String> {
    crate::daemon::check_completion_gates(work_dir, stage_id)
        .err()
        .map(|error| format!("completion gates: {error:#}"))
}
