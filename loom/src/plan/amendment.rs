//! Runtime plan amendment infrastructure.
//!
//! Plans are normally treated as immutable execution graphs. Adjudication
//! disputes that result in an `Accept` verdict require a way to mutate a
//! stage's acceptance criteria (or wiring) at runtime — atomically, with a
//! durable audit trail, and without losing in-flight runtime state.
//!
//! This module provides exactly that, subject to a tight contract:
//!
//! - **Scope:** only `acceptance`, `wiring`, `wiring_tests` and `contracts`
//!   arrays on a single stage are mutable. Everything else (stage IDs, dependencies,
//!   working_dir, plan structure) is off-limits.
//! - **Versioned + atomic:** under an `flock` on
//!   `.loom/work/plan_versions/.lock`, the amendment writes a numbered
//!   snapshot to `.loom/work/plan_versions/<n>.md`, appends one row to
//!   `.loom/work/plan_versions/audit.md`, replaces the live plan file via
//!   `safe_replace_outside_workdir`, AND rewrites the target stage's
//!   `.loom/work/stages/<n>-<id>.md`. Skipping the last step would silently
//!   leave the runtime reading the old criteria via
//!   `sync_graph_with_stage_files`.
//! - **Validated:** the proposed value is deserialized into the **real**
//!   [`AcceptanceCriterion`] / [`WiringCheck`] / [`WiringTest`] types before
//!   anything is written, so a malformed patch fails fast.
//! - **Capped:** a per-stage absolute amendment cap (default 3, override via
//!   `loom.adjudication.max_amendments_per_stage`) bounds runaway adjudication.
//!
//! Recovery semantics (called from orchestrator startup via
//! [`verify_plan_versions_consistency`]):
//!
//! - Snapshot written but audit row missing: snapshot is treated as orphaned;
//!   removed on startup so the next amendment can claim the same id.
//! - Audit row appended but plan file still old: re-apply just the amended
//!   field to the plan and stage file (catch-up commit); other edits survive.
//! - Plan + audit in sync but stage file stale: re-apply just the stage file
//!   update.
//! - `<n>.applied` certifies both writes. Live retries finish every pending
//!   audited field snapshot under the same lock before returning success.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{WiringCheck, WiringTest};
use crate::plan::amendment_catch_up;
use crate::plan::amendment_fields::{
    apply_patch_to_stage_def, current_field_len, parse_amendment_value, stage_field_matches,
    sync_stage_from_definition,
};
use crate::plan::parser::{extract_yaml_metadata_with_ranges, parse_and_validate};
use crate::plan::schema::{AcceptanceCriterion, ContractSpec, LoomMetadata};
use crate::verify::transitions::{load_stage, update_stage};

/// Which field of a stage is being amended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AmendmentField {
    /// Mutate the `acceptance` array.
    Acceptance,
    /// Mutate the `wiring` array.
    Wiring,
    /// Mutate the `wiring_tests` array.
    WiringTests,
    /// Mutate the `contracts` array (a v2 stage's behavioural contracts).
    Contracts,
}

fn field_from_name(name: &str) -> Option<AmendmentField> {
    crate::yaml::from_str(&format!("\"{}\"", name.replace('_', "-"))).ok()
}

/// The mutation to apply within the targeted field.
///
/// Patches operate on the field as an ordered list:
/// - `Replace` swaps the element at `index` with `value`.
/// - `Insert` inserts `value` at `index` (existing elements shift right).
/// - `Delete` removes the element at `index`.
///
/// `value` is YAML text; it is deserialized into the real
/// [`AcceptanceCriterion`] or [`WiringCheck`] before any I/O so a malformed
/// shape fails fast. `Delete` ignores the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum AmendmentPatch {
    /// Replace the element at `index` with the deserialized `value`.
    Replace {
        index: usize,
        /// YAML body for the new element. Deserialized into
        /// `AcceptanceCriterion` or `WiringCheck` depending on
        /// [`AmendmentField`].
        value: String,
    },
    /// Insert a new element at `index`, shifting existing elements right.
    Insert {
        index: usize,
        /// YAML body for the new element.
        value: String,
    },
    /// Remove the element at `index`.
    Delete { index: usize },
}

impl AmendmentPatch {
    /// The list position the patch targets.
    pub fn index(&self) -> usize {
        match self {
            AmendmentPatch::Replace { index, .. }
            | AmendmentPatch::Insert { index, .. }
            | AmendmentPatch::Delete { index } => *index,
        }
    }
}

/// A single amendment request targeting one stage and one field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentRequest {
    /// Target stage id (must exist in the plan).
    pub stage_id: String,
    /// Which array on the stage is being mutated.
    pub field: AmendmentField,
    /// The mutation to apply.
    pub patch: AmendmentPatch,
    /// Optional free-form reason recorded in the audit log.
    #[serde(default)]
    pub reason: Option<String>,
    /// Optional dispute id (request that triggered this amendment). Recorded
    /// in the audit log.
    #[serde(default)]
    pub dispute_id: Option<String>,
}

/// Outcome of a successful amendment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentResult {
    /// Monotonically-increasing version number assigned to this amendment.
    pub version: u64,
    /// Stage id that was amended.
    pub stage_id: String,
    /// Field that was amended.
    pub field: AmendmentField,
    /// Path written for the snapshot (`.loom/work/plan_versions/<n>.md`).
    pub snapshot_path: PathBuf,
    /// New value of `amendments_applied` on the stage AFTER this amendment.
    pub amendments_applied: u32,
    /// Timestamp the amendment was committed.
    pub applied_at: DateTime<Utc>,
}

const PLAN_VERSIONS_DIR_NAME: &str = "plan_versions";
const PLAN_VERSIONS_LOCK_NAME: &str = ".lock";
const AUDIT_FILE_NAME: &str = "audit.md";

/// Sentinel prefix used in the error returned when the per-stage amendment cap
/// has been reached. Callers (the orchestrator's `apply_verdict_inner`) match
/// on this prefix via [`is_amendment_cap_error`] to escalate the stage to
/// `NeedsHumanReview` instead of looping the verdict forever.
pub const AMENDMENT_CAP_ERROR_PREFIX: &str = "amendment cap exceeded";

/// True when `err` (or any source in its chain) carries the amendment-cap
/// sentinel produced by [`apply_amendment`].
pub fn is_amendment_cap_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|c| c.to_string().contains(AMENDMENT_CAP_ERROR_PREFIX))
}

/// Resolve the plan source path from `.loom/work/config.toml::source_path`,
/// canonicalising relative paths against the **main** project root (parent
/// of the resolved `.loom/work` dir).
fn resolve_plan_path(work_dir: &Path) -> Result<PathBuf> {
    let resolved = crate::fs::resolve_source_path(work_dir)
        .context("Failed to resolve plan source_path from .loom/work/config.toml")?;
    resolved.ok_or_else(|| {
        anyhow::anyhow!(
            "No plan source_path configured in .loom/work/config.toml at {}",
            work_dir.display()
        )
    })
}

/// Resolve the main project root that owns `work_dir`, following the
/// worktree symlink and applying the correct hop count for either
/// state-directory layout (`WorkDir::main_project_root` is the one place
/// that hop count lives — never recompute it here).
///
/// Falls back to `fallback`'s parent when `work_dir` cannot be resolved to a
/// workspace at all (e.g. it does not yet exist on disk in some tests).
fn resolve_project_root(work_dir: &Path, fallback: &Path) -> PathBuf {
    WorkDir::new(work_dir)
        .ok()
        .and_then(|wd| wd.main_project_root())
        .and_then(|root| root.canonicalize().ok())
        .unwrap_or_else(|| {
            fallback
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        })
}

/// Count the number of amendments prepared for `stage_id` by
/// reading the audit log. The audit log is the source of truth for the
/// per-stage amendment cap; it survives orchestrator restarts and is
/// independent of any in-memory Stage field.
pub fn count_amendments_for_stage(work_dir: &Path, stage_id: &str) -> Result<u32> {
    let rows = read_audit_rows(work_dir)?;
    let count = rows
        .iter()
        .filter(|r| r.stage_id == stage_id)
        .count()
        .min(u32::MAX as usize) as u32;
    Ok(count)
}

pub(super) enum ParsedAmendmentValue {
    Acceptance(AcceptanceCriterion),
    Wiring(WiringCheck),
    WiringTest(WiringTest),
    Contract(ContractSpec),
    None,
}

pub(super) fn serialize_loom_metadata(metadata: &LoomMetadata) -> Result<String> {
    crate::yaml::to_string(metadata).context("Failed to serialize loom metadata to YAML")
}

/// Splice a new YAML body into the original plan content, preserving every
/// byte outside the YAML fence (including the opening fence and language
/// hint, the closing fence, the metadata HTML comments, and all
/// human-readable prose).
pub(super) fn splice_metadata_yaml(
    original: &str,
    extracted: &crate::plan::parser::ExtractedMetadata,
    new_yaml_body: &str,
) -> String {
    // The YAML body sits between (fence_open + "yaml") and the closing
    // fence. Slice the original content into prefix / [fence_open + "yaml"]
    // / body / [closing fence + suffix].
    let fence_range = &extracted.yaml_fence_range;

    // Locate the start of the YAML body within fence_range.
    // The fence_range starts at the opening backticks. We need to know
    // where the body starts — that's `fence_range.start + fence_len +
    // "yaml".len()`. We can derive fence_len from the original content.
    let fence_open_bytes = &original.as_bytes()[fence_range.start..];
    let fence_len = fence_open_bytes.iter().take_while(|&&b| b == b'`').count();
    let body_start = fence_range.start + fence_len + "yaml".len();
    let body_end = fence_range.end;

    let prefix = &original[..body_start];
    let suffix = &original[body_end..];

    // The original YAML body was wrapped with leading/trailing newlines
    // (typically). Preserve a leading newline if present, otherwise add
    // one. Same for trailing.
    let mut spliced = String::with_capacity(original.len() + new_yaml_body.len());
    spliced.push_str(prefix);
    // Ensure a separating newline between "```yaml" and the body.
    if !spliced.ends_with('\n') {
        spliced.push('\n');
    }
    spliced.push_str(new_yaml_body.trim_end());
    if !spliced.ends_with('\n') {
        spliced.push('\n');
    }
    spliced.push_str(suffix);
    spliced
}

mod apply;
mod journal;
mod recovery;
#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod tests;
mod transaction;
pub use apply::apply_amendment;
pub use journal::plan_versions_dir;
use journal::*;
pub use recovery::verify_plan_versions_consistency;
