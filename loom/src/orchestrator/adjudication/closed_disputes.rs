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

use crate::models::dispute::{applied_marker, dispute_dir, CLOSED_MARKER};
use crate::verify::transitions::load_stage;

use super::apply_receipt::{cap_receipt, publish_applied_marker, verdict_receipt};

#[cfg(test)]
thread_local! {
    static CLOSE_FAILURE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn fail_close_once(dispute_id: u32) {
    CLOSE_FAILURE.with(|failure| failure.set(Some(dispute_id)));
}

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
#[cfg(test)]
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
    close_open_disputes_locked(work_dir, stage_id);
}

/// Caller already holds the stage's dispute lock, before any stage lock.
#[cfg(test)]
pub(crate) fn close_open_disputes_locked(work_dir: &Path, stage_id: &str) {
    let _ = close_open_disputes_except_locked(work_dir, stage_id, None);
}

/// Close siblings before publishing the current outcome marker. The current
/// dispute stays open so a crash before marker publication can replay it.
pub(crate) fn close_open_disputes_except_locked(
    work_dir: &Path,
    stage_id: &str,
    except: Option<u32>,
) -> anyhow::Result<()> {
    let disputes_root = work_dir.join("disputes");
    let stage_dir = disputes_root.join(stage_id);
    let stage = load_stage(stage_id, work_dir)?;
    let mut first_error = None;
    for entry in std::fs::read_dir(&stage_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if Some(id) == except || applied_marker(&disputes_root, stage_id, id).exists() {
            continue;
        }
        if stage
            .tally
            .request_receipts
            .contains_key(&verdict_receipt(id))
            || stage.tally.request_receipts.contains_key(&cap_receipt(id))
        {
            publish_applied_marker(work_dir, stage_id, id)?;
            continue;
        }
        if let Some(error) = write_closed_marker(&disputes_root, stage_id, id)? {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

fn write_closed_marker(
    disputes_root: &Path,
    stage_id: &str,
    id: u32,
) -> anyhow::Result<Option<std::io::Error>> {
    #[cfg(test)]
    if CLOSE_FAILURE.with(|failure| {
        if failure.get() == Some(id) {
            failure.set(None);
            true
        } else {
            false
        }
    }) {
        anyhow::bail!("injected dispute closure failure for {id}");
    }
    let marker = dispute_dir(disputes_root, stage_id, id).join(CLOSED_MARKER);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(None),
        Err(error) => {
            warn_not_closed(stage_id, Some(id), &error.to_string());
            Ok(Some(error))
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
