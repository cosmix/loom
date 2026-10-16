//! Closing the disputes a stage abandons when it escalates to a human or is
//! reset.
//!
//! An escalation out of the dispute loop leaves the stage's unanswered
//! disputes on disk, an apply-cap escalation leaves a verdict that never
//! applied, and `loom stage reset` leaves whatever was open. Left open, the
//! oldest of them shadows the dispute a fresh session files once the stage
//! runs again: disputes are scanned in ascending id order, so the abandoned
//! one would be judged again, or escalated again at its spent attempt cap,
//! ahead of the new one. A `closed.marker` in the
//! dispute's own directory settles it for good, and [`is_closed`] is the one
//! predicate every reader of dispute state consults.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Path;

use crate::models::dispute::{applied_marker, dispute_dir};

use super::scan::dispute_ids;

/// Zero-byte marker written into a closed dispute's directory.
const CLOSED_MARKER: &str = "closed.marker";

/// Whether the dispute whose directory is `dispute_dir` was closed. Any entry
/// under the marker's name counts, so a dangling link cannot reopen it.
pub(super) fn is_closed(dispute_dir: &Path) -> bool {
    std::fs::symlink_metadata(dispute_dir.join(CLOSED_MARKER)).is_ok()
}

pub(crate) use crate::fs::dispute_lock::lock_stage_dispute_dir;

/// Close every dispute of `stage_id` that has no applied outcome, under the
/// lock its filings take.
///
/// Called once the stage has left the dispute loop: after it is escalated to
/// `NeedsHumanReview` (and again when that review is approved), and by
/// `loom stage reset`. A stage still in the dispute loop must keep its
/// disputes open. Best effort: a failure is logged per dispute and the rest
/// are still closed.
pub(crate) fn close_open_disputes(work_dir: &Path, stage_id: &str) {
    let disputes_root = work_dir.join("disputes");
    let stage_dir = disputes_root.join(stage_id);
    // A stage never disputed has nothing to close, and taking the lock would
    // create `disputes/<stage>/.lock` for it.
    if !stage_dir.exists() {
        return;
    }
    let _lock = match lock_stage_dispute_dir(&stage_dir) {
        Ok(lock) => lock,
        Err(error) => {
            warn_not_closed(stage_id, None, &format!("{error:#}"));
            return;
        }
    };
    for id in dispute_ids(&stage_dir) {
        if applied_marker(&disputes_root, stage_id, id).exists() {
            continue;
        }
        let marker = dispute_dir(&disputes_root, stage_id, id).join(CLOSED_MARKER);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => warn_not_closed(stage_id, Some(id), &error.to_string()),
        }
    }
}

fn warn_not_closed(stage_id: &str, dispute_id: Option<u32>, error: &str) {
    tracing::warn!(
        target: "loom::adjudication",
        stage = %stage_id,
        dispute = ?dispute_id,
        %error,
        "could not close an open dispute; it may be judged again once the stage \
         runs again",
    );
}
