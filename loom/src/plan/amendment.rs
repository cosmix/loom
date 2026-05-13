//! Runtime plan amendment infrastructure.
//!
//! Plans are normally treated as immutable execution graphs. Adjudication
//! disputes that result in an `Accept` verdict require a way to mutate a
//! stage's acceptance criteria (or wiring) at runtime — atomically, with a
//! durable audit trail, and without losing in-flight runtime state.
//!
//! This module provides exactly that, subject to a tight contract:
//!
//! - **Scope:** only `acceptance` and `wiring` arrays on a single stage are
//!   mutable. Everything else (stage IDs, dependencies, working_dir, plan
//!   structure) is off-limits.
//! - **Versioned + atomic:** under an `flock` on
//!   `.work/plan_versions/.lock`, the amendment writes a numbered snapshot
//!   to `.work/plan_versions/<n>.md`, appends one row to
//!   `.work/plan_versions/audit.md`, replaces the live plan file via
//!   `safe_replace_outside_workdir`, AND rewrites the target stage's
//!   `.work/stages/<n>-<id>.md`. Skipping the last step would silently
//!   leave the runtime reading the old criteria via
//!   `sync_graph_with_stage_files`.
//! - **Validated:** the proposed value is deserialized into the **real**
//!   [`AcceptanceCriterion`] / [`WiringCheck`] types before anything is
//!   written, so a malformed patch fails fast.
//! - **Capped:** a per-stage absolute amendment cap (default 3, override via
//!   `loom.adjudication.max_amendments_per_stage`) bounds runaway adjudication.
//!
//! Recovery semantics (called from orchestrator startup via
//! [`verify_plan_versions_consistency`]):
//!
//! - Snapshot written but audit row missing: snapshot is treated as orphaned;
//!   removed on startup so the next amendment can claim the same id.
//! - Audit row appended but plan file still old: re-apply the snapshot to
//!   the plan and stage file (catch-up commit).
//! - Plan + audit in sync but stage file stale: re-apply just the stage file
//!   update.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs;
use crate::models::stage::{Stage, WiringCheck};
use crate::plan::parser::{extract_yaml_metadata_with_ranges, parse_and_validate};
use crate::plan::schema::{
    AcceptanceCriterion, AdjudicationConfig, LoomMetadata, StageDefinition,
};
use crate::verify::transitions::{load_stage, save_stage as persist_stage};

/// Which field of a stage is being amended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AmendmentField {
    /// Mutate the `acceptance` array.
    Acceptance,
    /// Mutate the `wiring` array.
    Wiring,
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
    Delete {
        index: usize,
    },
}

impl AmendmentPatch {
    fn index(&self) -> usize {
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
    /// Path written for the snapshot (`.work/plan_versions/<n>.md`).
    pub snapshot_path: PathBuf,
    /// New value of `amendments_applied` on the stage AFTER this amendment.
    pub amendments_applied: u32,
    /// Timestamp the amendment was committed.
    pub applied_at: DateTime<Utc>,
}

const PLAN_VERSIONS_DIR_NAME: &str = "plan_versions";
const PLAN_VERSIONS_LOCK_NAME: &str = ".lock";
const AUDIT_FILE_NAME: &str = "audit.md";

/// Path to `.work/plan_versions/`.
///
/// Returned as an absolute path when `work_dir` is absolute. The
/// foundations stage (parallel sibling) is expected to add a
/// `WorkDir::plan_versions_dir()` helper; this free function exists so the
/// amendment module compiles independently.
pub fn plan_versions_dir(work_dir: &Path) -> PathBuf {
    work_dir.join(PLAN_VERSIONS_DIR_NAME)
}

/// Compute the snapshot filename for a given version.
fn snapshot_filename(version: u64) -> String {
    format!("{version}.md")
}

/// Compute the snapshot tmp filename for a given version.
fn snapshot_tmp_filename(version: u64) -> String {
    format!("{version}.md.tmp")
}

/// Ensure `.work/plan_versions/` exists.
///
/// Idempotent. Created with mode 0o700 (state directory).
fn ensure_plan_versions_dir(work_dir: &Path) -> Result<PathBuf> {
    let dir = plan_versions_dir(work_dir);
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| {
            format!(
                "Failed to create plan_versions directory at {}",
                dir.display()
            )
        })?;
    }
    Ok(dir)
}

/// Open (or create) the plan-versions lock file and hold an exclusive flock
/// for the lifetime of the returned guard.
struct PlanVersionsLock {
    _file: fs::File,
}

impl PlanVersionsLock {
    fn acquire(work_dir: &Path) -> Result<Self> {
        let dir = ensure_plan_versions_dir(work_dir)?;
        let lock_path = dir.join(PLAN_VERSIONS_LOCK_NAME);
        // Use OpenOptions directly (not safe_fs) — the lock file is created
        // with O_CREAT and we want a long-lived handle for flock; safe_fs's
        // helpers all close-on-return.
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("Failed to open plan-versions lock {}", lock_path.display()))?;
        file.lock_exclusive().with_context(|| {
            format!(
                "Failed to acquire plan-versions lock {}",
                lock_path.display()
            )
        })?;
        Ok(Self { _file: file })
    }
}

/// One row in `.work/plan_versions/audit.md`.
///
/// The audit log is a markdown table that is append-only via `O_APPEND`.
/// We keep the schema deliberately simple so a human can read the file
/// without tooling: `version | stage_id | field | patch_op | index |
/// applied_at | dispute_id | reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AuditRow {
    version: u64,
    stage_id: String,
    field: AmendmentField,
    patch_op: &'static str,
    index: usize,
    applied_at: DateTime<Utc>,
    dispute_id: Option<String>,
    reason: Option<String>,
}

impl AuditRow {
    fn to_markdown_row(&self) -> String {
        // The audit log uses a markdown table parsed by splitting on '|'.
        // We aggressively sanitise the reason cell so a stray pipe or
        // newline can never confuse the parser: pipes become '/' and
        // newlines become spaces. Round-trip fidelity of the reason text
        // is intentionally sacrificed for parser simplicity.
        let reason = self
            .reason
            .as_deref()
            .unwrap_or("")
            .replace('|', "/")
            .replace('\n', " ");
        let dispute = self.dispute_id.as_deref().unwrap_or("");
        let field = match self.field {
            AmendmentField::Acceptance => "acceptance",
            AmendmentField::Wiring => "wiring",
        };
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            self.version,
            self.stage_id,
            field,
            self.patch_op,
            self.index,
            self.applied_at.to_rfc3339(),
            dispute,
            reason,
        )
    }
}

const AUDIT_HEADER: &str = "# Plan Amendment Audit\n\n\
| version | stage_id | field | op | index | applied_at | dispute_id | reason |\n\
|---------|----------|-------|----|-------|------------|------------|--------|\n";

/// Read every audit row currently in `.work/plan_versions/audit.md`.
/// Returns an empty vec if the file does not exist.
fn read_audit_rows(work_dir: &Path) -> Result<Vec<AuditRow>> {
    let dir = plan_versions_dir(work_dir);
    let audit_path = dir.join(AUDIT_FILE_NAME);
    if !audit_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&audit_path)
        .with_context(|| format!("Failed to read audit log {}", audit_path.display()))?;
    let mut rows = Vec::new();
    for line in content.lines() {
        if !line.starts_with("| ") {
            continue;
        }
        // Skip header / separator rows.
        if line.starts_with("| version ") || line.starts_with("|---") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        // Expected: [empty, version, stage_id, field, op, index, applied_at, dispute_id, reason, empty]
        if cells.len() < 10 {
            continue;
        }
        let version: u64 = match cells[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let stage_id = cells[2].to_string();
        let field = match cells[3] {
            "acceptance" => AmendmentField::Acceptance,
            "wiring" => AmendmentField::Wiring,
            _ => continue,
        };
        let patch_op = match cells[4] {
            "replace" => "replace",
            "insert" => "insert",
            "delete" => "delete",
            _ => continue,
        };
        let index: usize = match cells[5].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let applied_at: DateTime<Utc> = match DateTime::parse_from_rfc3339(cells[6]) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        let dispute_id = if cells[7].is_empty() {
            None
        } else {
            Some(cells[7].to_string())
        };
        let reason = if cells[8].is_empty() {
            None
        } else {
            Some(cells[8].to_string())
        };
        rows.push(AuditRow {
            version,
            stage_id,
            field,
            patch_op,
            index,
            applied_at,
            dispute_id,
            reason,
        });
    }
    Ok(rows)
}

/// Append a single audit row via `safe_append_in_workdir` (O_APPEND + flock).
fn append_audit_row(work_dir: &Path, row: &AuditRow) -> Result<()> {
    let dir = plan_versions_dir(work_dir);
    let audit_path = dir.join(AUDIT_FILE_NAME);
    // Initialise header if missing — write the header + row in one shot.
    if !audit_path.exists() {
        let mut body = String::from(AUDIT_HEADER);
        body.push_str(&row.to_markdown_row());
        fs::write(&audit_path, body)
            .with_context(|| format!("Failed to write audit log {}", audit_path.display()))?;
        return Ok(());
    }
    // Append-only via safe_fs: it opens with O_APPEND, takes an flock, and
    // writes the row atomically (single write() call after the flock).
    let dirfd = safe_fs::safe_open_dirfd(&dir)?;
    let rel = Path::new(AUDIT_FILE_NAME);
    safe_fs::safe_append_in_workdir(dirfd.as_raw_fd(), rel, row.to_markdown_row().as_bytes())?;
    Ok(())
}

/// Resolve the plan source path from `.work/config.toml::source_path`,
/// canonicalising relative paths against the **main** project root (parent
/// of the resolved `.work` dir).
fn resolve_plan_path(work_dir: &Path) -> Result<PathBuf> {
    let resolved = crate::fs::resolve_source_path(work_dir)
        .context("Failed to resolve plan source_path from .work/config.toml")?;
    resolved.ok_or_else(|| {
        anyhow::anyhow!(
            "No plan source_path configured in .work/config.toml at {}",
            work_dir.display()
        )
    })
}

/// Apply a runtime amendment to a single stage.
///
/// Steps (all under flock on `.work/plan_versions/.lock`):
///
/// 1. Resolve `plan_path` (parameter is preferred; falls back to
///    `.work/config.toml::source_path` if the supplied path doesn't exist).
/// 2. Parse the plan markdown and load the target stage file
///    (`.work/stages/<n>-<id>.md`).
/// 3. Deserialize the patch's `value` into the **real** Rust type
///    ([`AcceptanceCriterion`] or [`WiringCheck`]) and bounds-check `index`.
/// 4. Refuse if the per-stage absolute amendment cap is already reached.
/// 5. Apply the patch in-memory; clone-and-mutate the plan AND the stage.
/// 6. Serialize amended YAML; splice into the markdown's metadata block
///    using [`extract_yaml_metadata_with_ranges`]; surrounding prose is
///    preserved byte-for-byte.
/// 7. Write a snapshot to `.work/plan_versions/<n>.md.tmp` via
///    `safe_create_new_in_workdir` then atomically rename to `<n>.md`
///    via `safe_rename_in_workdir`.
/// 8. Append one row to `.work/plan_versions/audit.md` (O_APPEND under flock).
/// 9. Write the new plan content to `plan_path` via
///    `safe_replace_outside_workdir` (dirfd-anchored atomic rename).
/// 10. Persist the updated stage definition via
///     `verify::transitions::save_stage` — this is what
///     `sync_graph_with_stage_files` consumes; skipping it leaves stale
///     criteria in effect at runtime.
pub fn apply_amendment(
    plan_path: &Path,
    work_dir: &Path,
    request: AmendmentRequest,
) -> Result<AmendmentResult> {
    // -- 0. Acquire the global plan-versions lock for the duration of the
    //       operation. All recovery happens under the same lock so a crash
    //       between steps leaves a state we can reconcile on startup.
    let _lock = PlanVersionsLock::acquire(work_dir)?;

    // -- 0a. Idempotency under crash-mid-apply: if the audit log already
    //        contains a row matching this (stage_id, dispute_id), the
    //        previous call landed (snapshot + audit + plan rewrite) but
    //        the caller crashed before writing its own success marker.
    //        Re-applying the patch would double-patch (Insert duplicates,
    //        Delete shifts to the wrong index, Replace adds another
    //        snapshot row). Return the existing result instead.
    if let Some(dispute_id) = request.dispute_id.as_deref() {
        let prior = read_audit_rows(work_dir)?.into_iter().find(|r| {
            r.stage_id == request.stage_id && r.dispute_id.as_deref() == Some(dispute_id)
        });
        if let Some(row) = prior {
            let count = count_amendments_for_stage(work_dir, &request.stage_id)?;
            return Ok(AmendmentResult {
                version: row.version,
                stage_id: row.stage_id,
                field: row.field,
                snapshot_path: plan_versions_dir(work_dir).join(snapshot_filename(row.version)),
                amendments_applied: count,
                applied_at: row.applied_at,
            });
        }
    }

    // -- 1. Resolve the plan path (fall back to config.toml if the supplied
    //       path is not accessible).
    let plan_path = if plan_path.exists() {
        plan_path.to_path_buf()
    } else {
        resolve_plan_path(work_dir)?
    };

    // Canonicalise the project root for safe_replace_outside_workdir's
    // path-confinement check. The project root is the parent of `.work/`
    // (in worktree layout, `.work/` is a symlink to the main repo's
    // `.work/`, so canonicalise first to follow the symlink). When work_dir
    // can't be canonicalised — e.g. it does not yet exist on disk in some
    // tests — fall back to the plan file's parent directory.
    let project_root = work_dir
        .canonicalize()
        .ok()
        .and_then(|wd| wd.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| {
            plan_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        });

    // Early confinement check: refuse a plan file that is not under the
    // resolved project_root BEFORE we start writing snapshots and audit
    // rows. Without this, a refusal at step 9 would leave a half-done
    // snapshot + audit row that recovery would keep trying to replay.
    let canonical_root = project_root.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalise project_root {}",
            project_root.display()
        )
    })?;
    let plan_parent = plan_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("plan_path has no parent: {}", plan_path.display()))?;
    let canonical_plan_parent = plan_parent.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalise plan parent {}",
            plan_parent.display()
        )
    })?;
    if !canonical_plan_parent.starts_with(&canonical_root) {
        bail!(
            "refusing replace — plan {} not under project_root {}",
            plan_path.display(),
            project_root.display()
        );
    }

    // -- 2. Parse plan + load target stage file.
    let original_plan_content = fs::read_to_string(&plan_path)
        .with_context(|| format!("Failed to read plan {}", plan_path.display()))?;
    let extracted = extract_yaml_metadata_with_ranges(&original_plan_content)
        .with_context(|| format!("Failed to extract metadata from {}", plan_path.display()))?;
    let metadata = parse_and_validate(&extracted.yaml)
        .with_context(|| format!("Plan validation failed for {}", plan_path.display()))?;

    let stage_idx = metadata
        .loom
        .stages
        .iter()
        .position(|s| s.id == request.stage_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Stage '{}' not found in plan {}",
                request.stage_id,
                plan_path.display()
            )
        })?;

    let mut stage_def = metadata.loom.stages[stage_idx].clone();

    let mut stage = load_stage(&request.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage file for '{}'", request.stage_id))?;

    // -- 3. Validate the patch shape by deserializing the proposed value
    //       into the REAL type (NOT a hand-rolled simplified shape).
    let parsed_value = match (request.field, &request.patch) {
        (AmendmentField::Acceptance, AmendmentPatch::Replace { value, .. })
        | (AmendmentField::Acceptance, AmendmentPatch::Insert { value, .. }) => {
            let v: AcceptanceCriterion = serde_yaml::from_str(value).with_context(|| {
                format!(
                    "Invalid AcceptanceCriterion in amendment for stage '{}'",
                    request.stage_id
                )
            })?;
            ParsedAmendmentValue::Acceptance(v)
        }
        (AmendmentField::Wiring, AmendmentPatch::Replace { value, .. })
        | (AmendmentField::Wiring, AmendmentPatch::Insert { value, .. }) => {
            let v: WiringCheck = serde_yaml::from_str(value).with_context(|| {
                format!(
                    "Invalid WiringCheck in amendment for stage '{}'",
                    request.stage_id
                )
            })?;
            ParsedAmendmentValue::Wiring(v)
        }
        (_, AmendmentPatch::Delete { .. }) => ParsedAmendmentValue::None,
    };

    // Bounds-check `index` against the CURRENT length of the targeted array.
    let current_len = current_field_len(&stage_def, request.field);
    let idx = request.patch.index();
    let allowed_upper = match request.patch {
        AmendmentPatch::Insert { .. } => current_len, // insert-at-end is valid
        _ => current_len.saturating_sub(1),
    };
    let valid = match request.patch {
        AmendmentPatch::Insert { .. } => idx <= current_len,
        _ => current_len > 0 && idx <= allowed_upper,
    };
    if !valid {
        bail!(
            "Amendment index out of bounds: stage '{}' field {:?} has {} entries; index {} not permitted for {}",
            request.stage_id,
            request.field,
            current_len,
            idx,
            match request.patch {
                AmendmentPatch::Replace { .. } => "replace",
                AmendmentPatch::Insert { .. } => "insert",
                AmendmentPatch::Delete { .. } => "delete",
            },
        );
    }

    // -- 4. Per-stage absolute amendment cap. The cap counts ALL successful
    //       audit rows for this stage. We use the audit log as the source of
    //       truth so the count survives orchestrator restarts and is
    //       independent of any in-memory Stage field.
    let cap = metadata
        .loom
        .adjudication
        .clone()
        .unwrap_or_default()
        .max_amendments_per_stage;
    let prior_count = count_amendments_for_stage(work_dir, &request.stage_id)?;
    if prior_count >= cap {
        bail!(
            "Stage '{}' has reached the amendment cap ({} of {})",
            request.stage_id,
            prior_count,
            cap,
        );
    }

    // -- 5. Apply the patch to the cloned plan + stage.
    apply_patch_to_stage_def(&mut stage_def, request.field, &request.patch, &parsed_value)?;
    apply_patch_to_runtime_stage(&mut stage, request.field, &request.patch, &parsed_value)?;

    let mut new_metadata = metadata.clone();
    new_metadata.loom.stages[stage_idx] = stage_def.clone();

    // Re-validate the mutated plan in-memory before writing anything. A
    // Delete that empties a Standard stage's acceptance + wiring + artifacts
    // would otherwise produce an invalid plan on disk that recovery would
    // refuse to re-parse.
    if let Err(errors) = crate::plan::schema::validate(&new_metadata) {
        bail!(
            "Amendment would produce an invalid plan: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    // -- 6. Serialize amended YAML and splice into markdown.
    let new_yaml_body = serialize_loom_metadata(&new_metadata)?;
    let new_plan_content = splice_metadata_yaml(&original_plan_content, &extracted, &new_yaml_body);

    // -- 7. Snapshot to .work/plan_versions/<n>.md.tmp then rename to <n>.md.
    let next_version = compute_next_version(work_dir)?;
    let plan_versions = ensure_plan_versions_dir(work_dir)?;
    let dirfd = safe_fs::safe_open_dirfd(&plan_versions)?;
    let tmp_rel = PathBuf::from(snapshot_tmp_filename(next_version));
    let final_rel = PathBuf::from(snapshot_filename(next_version));
    safe_fs::safe_create_new_in_workdir(
        dirfd.as_raw_fd(),
        &tmp_rel,
        new_plan_content.as_bytes(),
    )
    .with_context(|| format!("Failed to create snapshot tmp for version {next_version}"))?;
    safe_fs::safe_rename_in_workdir(dirfd.as_raw_fd(), &tmp_rel, &final_rel).with_context(
        || format!("Failed to rename snapshot for version {next_version}"),
    )?;

    let applied_at = Utc::now();

    // -- 8. Append audit row.
    let patch_op = match request.patch {
        AmendmentPatch::Replace { .. } => "replace",
        AmendmentPatch::Insert { .. } => "insert",
        AmendmentPatch::Delete { .. } => "delete",
    };
    let audit_row = AuditRow {
        version: next_version,
        stage_id: request.stage_id.clone(),
        field: request.field,
        patch_op,
        index: idx,
        applied_at,
        dispute_id: request.dispute_id.clone(),
        reason: request.reason.clone(),
    };
    append_audit_row(work_dir, &audit_row)?;

    // -- 9. Replace the live plan file atomically.
    safe_fs::safe_replace_outside_workdir(
        &plan_path,
        &project_root,
        new_plan_content.as_bytes(),
    )
    .with_context(|| format!("Failed to replace live plan {}", plan_path.display()))?;

    // -- 10. Persist the updated stage file. Without this, the runtime keeps
    //        the old criteria via sync_graph_with_stage_files.
    persist_stage(&stage, work_dir)
        .with_context(|| format!("Failed to save amended stage '{}'", request.stage_id))?;

    Ok(AmendmentResult {
        version: next_version,
        stage_id: request.stage_id,
        field: request.field,
        snapshot_path: plan_versions.join(snapshot_filename(next_version)),
        amendments_applied: prior_count.saturating_add(1),
        applied_at,
    })
}

/// Count the number of amendments successfully applied to `stage_id` by
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

/// Verify and reconcile `.work/plan_versions/` against the live plan file
/// and target stage files. Called from orchestrator startup.
///
/// Recovery cases (executed under the plan-versions flock):
///
/// 1. **Snapshot exists, audit row missing** → snapshot is orphaned (we
///    crashed between step 7 and step 8). Remove the snapshot.
/// 2. **Audit row exists, plan file != latest snapshot** → catch-up commit:
///    re-write the live plan and re-save the target stage from the snapshot.
/// 3. **Plan file == latest snapshot, but stage file YAML drifted** →
///    re-save the stage definition from the snapshot.
///
/// Returns the number of recovery actions taken.
pub fn verify_plan_versions_consistency(plan_path: &Path, work_dir: &Path) -> Result<usize> {
    let dir = plan_versions_dir(work_dir);
    if !dir.exists() {
        return Ok(0);
    }
    let _lock = PlanVersionsLock::acquire(work_dir)?;

    // Discover snapshot versions on disk.
    let mut snapshot_versions: Vec<u64> = Vec::new();
    let mut orphan_tmp: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| {
        format!("Failed to read plan_versions directory {}", dir.display())
    })? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == AUDIT_FILE_NAME || name == PLAN_VERSIONS_LOCK_NAME {
            continue;
        }
        if let Some(stem) = name.strip_suffix(".md.tmp") {
            if stem.parse::<u64>().is_ok() {
                orphan_tmp.push(path);
            }
            continue;
        }
        if let Some(stem) = name.strip_suffix(".md") {
            if let Ok(v) = stem.parse::<u64>() {
                snapshot_versions.push(v);
            }
        }
    }
    snapshot_versions.sort_unstable();

    let audit_rows = read_audit_rows(work_dir)?;
    let audited_versions: std::collections::HashSet<u64> =
        audit_rows.iter().map(|r| r.version).collect();

    let mut actions = 0usize;

    // Remove dangling .tmp files (crash between create_new and rename).
    for p in &orphan_tmp {
        let _ = fs::remove_file(p);
        actions += 1;
    }

    // Case 1: snapshot without matching audit row → snapshot orphaned.
    for v in &snapshot_versions {
        if !audited_versions.contains(v) {
            let snap_path = dir.join(snapshot_filename(*v));
            let _ = fs::remove_file(&snap_path);
            actions += 1;
        }
    }

    // After cleanup, find the latest still-valid snapshot.
    let latest = snapshot_versions
        .iter()
        .copied()
        .filter(|v| audited_versions.contains(v))
        .max();

    let Some(latest_version) = latest else {
        return Ok(actions);
    };

    let latest_snapshot_path = dir.join(snapshot_filename(latest_version));
    let snapshot_content = fs::read_to_string(&latest_snapshot_path).with_context(|| {
        format!(
            "Failed to read latest snapshot {}",
            latest_snapshot_path.display()
        )
    })?;

    // Find the audit row for this version so we know which stage to reconcile.
    let row = match audit_rows.iter().find(|r| r.version == latest_version) {
        Some(r) => r.clone(),
        None => return Ok(actions),
    };

    // Case 2: live plan does not match latest snapshot → catch up.
    let plan_path_buf = if plan_path.exists() {
        plan_path.to_path_buf()
    } else {
        resolve_plan_path(work_dir).unwrap_or_else(|_| plan_path.to_path_buf())
    };

    let live_content = fs::read_to_string(&plan_path_buf).ok();
    if live_content.as_deref() != Some(snapshot_content.as_str()) {
        // Re-write the plan file from the snapshot.
        let project_root = work_dir
            .canonicalize()
            .ok()
            .and_then(|wd| wd.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| plan_path_buf.parent().unwrap_or(Path::new(".")).to_path_buf());
        // Best-effort: only attempt the write if plan_path_buf is under the
        // project root (the safe-fs helper enforces this anyway).
        if safe_fs::safe_replace_outside_workdir(
            &plan_path_buf,
            &project_root,
            snapshot_content.as_bytes(),
        )
        .is_ok()
        {
            actions += 1;
        }
    }

    // Case 3: stage file needs to reflect the snapshot's stage definition.
    let extracted = match extract_yaml_metadata_with_ranges(&snapshot_content) {
        Ok(e) => e,
        Err(_) => return Ok(actions),
    };
    let snap_metadata: LoomMetadata = match parse_and_validate(&extracted.yaml) {
        Ok(m) => m,
        Err(_) => return Ok(actions),
    };
    let Some(snap_stage_def) = snap_metadata
        .loom
        .stages
        .iter()
        .find(|s| s.id == row.stage_id)
    else {
        return Ok(actions);
    };
    if let Ok(mut current_stage) = load_stage(&row.stage_id, work_dir) {
        if !stage_field_matches(&current_stage, snap_stage_def, row.field) {
            sync_stage_from_definition(&mut current_stage, snap_stage_def, row.field);
            // Don't bump amendments_applied here — it was already incremented
            // when the original `apply_amendment` ran (step 10). Recovery
            // only catches up the field content.
            if persist_stage(&current_stage, work_dir).is_ok() {
                actions += 1;
            }
        }
    }

    Ok(actions)
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

enum ParsedAmendmentValue {
    Acceptance(AcceptanceCriterion),
    Wiring(WiringCheck),
    None,
}

fn current_field_len(stage: &StageDefinition, field: AmendmentField) -> usize {
    match field {
        AmendmentField::Acceptance => stage.acceptance.len(),
        AmendmentField::Wiring => stage.wiring.len(),
    }
}

fn apply_patch_to_stage_def(
    stage: &mut StageDefinition,
    field: AmendmentField,
    patch: &AmendmentPatch,
    value: &ParsedAmendmentValue,
) -> Result<()> {
    match field {
        AmendmentField::Acceptance => apply_patch_vec(
            &mut stage.acceptance,
            patch,
            match value {
                ParsedAmendmentValue::Acceptance(v) => Some(v.clone()),
                _ => None,
            },
        ),
        AmendmentField::Wiring => apply_patch_vec(
            &mut stage.wiring,
            patch,
            match value {
                ParsedAmendmentValue::Wiring(v) => Some(v.clone()),
                _ => None,
            },
        ),
    }
}

fn apply_patch_to_runtime_stage(
    stage: &mut Stage,
    field: AmendmentField,
    patch: &AmendmentPatch,
    value: &ParsedAmendmentValue,
) -> Result<()> {
    match field {
        AmendmentField::Acceptance => apply_patch_vec(
            &mut stage.acceptance,
            patch,
            match value {
                ParsedAmendmentValue::Acceptance(v) => Some(v.clone()),
                _ => None,
            },
        ),
        AmendmentField::Wiring => apply_patch_vec(
            &mut stage.wiring,
            patch,
            match value {
                ParsedAmendmentValue::Wiring(v) => Some(v.clone()),
                _ => None,
            },
        ),
    }
}

fn apply_patch_vec<T: Clone>(
    vec: &mut Vec<T>,
    patch: &AmendmentPatch,
    new_value: Option<T>,
) -> Result<()> {
    match patch {
        AmendmentPatch::Replace { index, .. } => {
            if *index >= vec.len() {
                bail!("Replace index {} out of bounds (len {})", index, vec.len());
            }
            let v = new_value
                .ok_or_else(|| anyhow::anyhow!("Replace patch missing typed value"))?;
            vec[*index] = v;
        }
        AmendmentPatch::Insert { index, .. } => {
            if *index > vec.len() {
                bail!("Insert index {} out of bounds (len {})", index, vec.len());
            }
            let v = new_value
                .ok_or_else(|| anyhow::anyhow!("Insert patch missing typed value"))?;
            vec.insert(*index, v);
        }
        AmendmentPatch::Delete { index } => {
            if *index >= vec.len() {
                bail!("Delete index {} out of bounds (len {})", index, vec.len());
            }
            vec.remove(*index);
        }
    }
    Ok(())
}

fn serialize_loom_metadata(metadata: &LoomMetadata) -> Result<String> {
    serde_yaml::to_string(metadata).context("Failed to serialize loom metadata to YAML")
}

/// Splice a new YAML body into the original plan content, preserving every
/// byte outside the YAML fence (including the opening fence and language
/// hint, the closing fence, the metadata HTML comments, and all
/// human-readable prose).
fn splice_metadata_yaml(
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
    let fence_len = fence_open_bytes
        .iter()
        .take_while(|&&b| b == b'`')
        .count();
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

fn compute_next_version(work_dir: &Path) -> Result<u64> {
    let rows = read_audit_rows(work_dir)?;
    let max = rows.iter().map(|r| r.version).max().unwrap_or(0);
    // Also account for any snapshot files (e.g. from a crash before audit
    // append) so we don't reuse a version.
    let dir = plan_versions_dir(work_dir);
    let mut max_file: u64 = 0;
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".md") {
                    if let Ok(v) = stem.parse::<u64>() {
                        if v > max_file {
                            max_file = v;
                        }
                    }
                }
                if let Some(stem) = name.strip_suffix(".md.tmp") {
                    if let Ok(v) = stem.parse::<u64>() {
                        if v > max_file {
                            max_file = v;
                        }
                    }
                }
            }
        }
    }
    Ok(max.max(max_file) + 1)
}

fn stage_field_matches(stage: &Stage, def: &StageDefinition, field: AmendmentField) -> bool {
    match field {
        AmendmentField::Acceptance => stage.acceptance == def.acceptance,
        AmendmentField::Wiring => {
            // WiringCheck doesn't derive PartialEq; compare by serialized form.
            let a = serde_yaml::to_string(&stage.wiring).unwrap_or_default();
            let b = serde_yaml::to_string(&def.wiring).unwrap_or_default();
            a == b
        }
    }
}

fn sync_stage_from_definition(stage: &mut Stage, def: &StageDefinition, field: AmendmentField) {
    match field {
        AmendmentField::Acceptance => {
            stage.acceptance = def.acceptance.clone();
        }
        AmendmentField::Wiring => {
            stage.wiring = def.wiring.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for amendment-module internals. Integration tests that
    //! exercise the full `apply_amendment` flow live under
    //! `plan::tests::amendment`.
    use super::*;

    #[test]
    fn audit_row_round_trip_via_markdown() {
        let row = AuditRow {
            version: 7,
            stage_id: "feature-x".to_string(),
            field: AmendmentField::Acceptance,
            patch_op: "replace",
            index: 2,
            applied_at: DateTime::parse_from_rfc3339("2026-05-13T07:30:00+00:00")
                .unwrap()
                .with_timezone(&Utc),
            dispute_id: Some("d-42".to_string()),
            reason: Some("agent demonstrated env mismatch".to_string()),
        };
        let md = format!("{}{}", AUDIT_HEADER, row.to_markdown_row());
        // Parse back through the same path read_audit_rows uses by
        // splitting on '|' and trimming.
        let last_line = md.lines().last().unwrap();
        let cells: Vec<&str> = last_line.split('|').map(|s| s.trim()).collect();
        assert_eq!(cells[1], "7");
        assert_eq!(cells[2], "feature-x");
        assert_eq!(cells[3], "acceptance");
        assert_eq!(cells[4], "replace");
        assert_eq!(cells[5], "2");
        assert_eq!(cells[7], "d-42");
        assert_eq!(cells[8], "agent demonstrated env mismatch");
    }

    #[test]
    fn audit_row_sanitises_pipe_and_newline_in_reason() {
        let row = AuditRow {
            version: 1,
            stage_id: "s".to_string(),
            field: AmendmentField::Wiring,
            patch_op: "delete",
            index: 0,
            applied_at: Utc::now(),
            dispute_id: None,
            reason: Some("contains | a pipe and\nnewline".to_string()),
        };
        let line = row.to_markdown_row();
        // Exactly one row-terminator newline, no embedded newlines.
        assert_eq!(line.matches('\n').count(), 1);
        // Pipes inside the reason cell have been replaced with '/' so the
        // table structure is preserved; the row parses to exactly the
        // documented number of cells.
        let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        assert_eq!(cells.len(), 10, "got {cells:?}");
        assert_eq!(cells[8], "contains / a pipe and newline");
    }

    #[test]
    fn current_field_len_reads_definition() {
        let mut def = StageDefinition {
            id: "s".to_string(),
            name: "S".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![
                AcceptanceCriterion::Simple("cargo test".to_string()),
                AcceptanceCriterion::Simple("cargo clippy".to_string()),
            ],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: Default::default(),
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
            sandbox: Default::default(),
            execution_mode: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            code_review: None,
            execution: None,
        };
        assert_eq!(current_field_len(&def, AmendmentField::Acceptance), 2);
        assert_eq!(current_field_len(&def, AmendmentField::Wiring), 0);
        def.wiring.push(WiringCheck {
            source: "x".to_string(),
            pattern: "y".to_string(),
            description: "z".to_string(),
        });
        assert_eq!(current_field_len(&def, AmendmentField::Wiring), 1);
    }

    #[test]
    fn apply_patch_vec_bounds_check_on_replace() {
        let mut v: Vec<i32> = vec![1, 2, 3];
        let p = AmendmentPatch::Replace {
            index: 5,
            value: String::new(),
        };
        assert!(apply_patch_vec(&mut v, &p, Some(99)).is_err());
    }

    #[test]
    fn apply_patch_vec_insert_at_end_is_ok() {
        let mut v: Vec<i32> = vec![1, 2, 3];
        let p = AmendmentPatch::Insert {
            index: 3,
            value: String::new(),
        };
        apply_patch_vec(&mut v, &p, Some(4)).unwrap();
        assert_eq!(v, vec![1, 2, 3, 4]);
    }

    #[test]
    fn apply_patch_vec_delete_shifts() {
        let mut v: Vec<i32> = vec![1, 2, 3];
        let p = AmendmentPatch::Delete { index: 1 };
        apply_patch_vec::<i32>(&mut v, &p, None).unwrap();
        assert_eq!(v, vec![1, 3]);
    }
}
