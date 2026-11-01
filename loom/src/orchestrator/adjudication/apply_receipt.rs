//! Durable verdict receipt and secondary marker publication.

use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::path::Path;

use crate::models::dispute::applied_marker;
use crate::models::stage::Stage;

pub(super) fn verdict_receipt(dispute_id: u32) -> String {
    // Request idempotency keys pass validate_id, which rejects ':'. The
    // namespace cannot collide with a caller-supplied request key.
    format!("adjudication-verdict:{dispute_id}")
}

pub(super) fn cap_receipt(dispute_id: u32) -> String {
    format!("adjudication-apply-cap:{dispute_id}")
}

pub(super) fn publish_applied_marker(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
) -> Result<()> {
    let path = applied_marker(&work_dir.join("disputes"), stage_id, dispute_id);
    write_marker(&path)
}

/// A marker failure leaves this dispute open for receipt-based cap recovery.
pub(super) fn finish_apply_cap(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    failures: u32,
    error: &anyhow::Error,
    claim: &OwnedFd,
) -> Result<()> {
    super::escalate_apply_cap_claimed(work_dir, stage_id, dispute_id, failures, error, claim)?;
    super::close_open_disputes_except_locked(work_dir, stage_id, Some(dispute_id))?;
    publish_applied_marker(work_dir, stage_id, dispute_id)?;
    Ok(())
}

pub(super) fn write_marker(path: &Path) -> Result<()> {
    #[cfg(test)]
    if MARKER_FAILURE.with(|fail| fail.replace(false)) {
        anyhow::bail!("injected applied-marker write failure");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, b"").with_context(|| format!("Failed to write {}", path.display()))?;
    // An old best-effort marker has no claim or recovery role.
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_file(parent.join(".applying"));
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static MARKER_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn fail_marker_once() {
    MARKER_FAILURE.with(|fail| fail.set(true));
}

/// Apply only a reason changed by this verdict, preserving a newer sibling
/// reason if the verdict left its loaded reason untouched.
pub(super) fn apply_reason_change(stage: &mut Stage, loaded: Option<&str>, verdict: Option<&str>) {
    if verdict == loaded {
        return;
    }
    match verdict {
        Some(reason) => stage.replace_review_reason(reason.to_owned()),
        None if stage.review_reason.as_deref() == loaded => {
            stage.retire_review_reason(
                "adjudicator verdict applied",
                crate::models::stage::IssueDisposition::Resolved,
            );
        }
        None => {}
    }
}
