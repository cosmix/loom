//! Field-scoped catch-up for `verify_plan_versions_consistency`'s Case 2.
//!
//! Case 2 used to restore the entire live plan file from the latest
//! amendment snapshot whenever the two differed byte-for-byte. That reverted
//! every legitimate edit made to the plan after the amendment — prose,
//! another stage's fields, anything — any time the daemon restarted. This
//! module narrows both the comparison and the write to exactly the
//! `(stage_id, field)` the audit row records as amended, mirroring how Case
//! 3 already treats the stage file.

use std::fs;
use std::path::Path;

use crate::fs::safe_fs;
use crate::plan::parser::{
    extract_yaml_metadata_with_ranges, parse_and_validate, ExtractedMetadata,
};
use crate::plan::schema::{LoomMetadata, StageDefinition};

use super::amendment::{serialize_loom_metadata, splice_metadata_yaml, AmendmentField};

/// Reconcile the live plan's `(stage_id, field)` against the snapshot's
/// value for that field, leaving everything else in the live file alone.
///
/// `Ok(true)` means the live plan was rewritten; `Ok(false)` means the field
/// already matched and nothing else was touched. `Err` carries the reason
/// the reconciliation had to be skipped without writing anything — the
/// caller logs it via `warn_skip`.
pub(super) fn reconcile_amended_field(
    plan_path: &Path,
    project_root: &Path,
    snapshot_content: &str,
    stage_id: &str,
    field: AmendmentField,
) -> Result<bool, &'static str> {
    let live_content = fs::read_to_string(plan_path).map_err(|_| "cannot read live plan")?;
    let (extracted, mut live_metadata) = parse_live_plan(&live_content)?;
    let snap_stage = find_stage_def(snapshot_content, stage_id)
        .ok_or("stage missing from snapshot, or snapshot metadata unparseable")?;
    let live_idx = live_metadata
        .loom
        .stages
        .iter()
        .position(|s| s.id == stage_id)
        .ok_or("stage missing from live plan")?;

    if stage_def_field_matches(&live_metadata.loom.stages[live_idx], &snap_stage, field) {
        // Already applied. Every other difference between the live file and
        // the snapshot is a legitimate edit made since — leave it alone.
        return Ok(false);
    }
    copy_field(&mut live_metadata.loom.stages[live_idx], &snap_stage, field);

    write_reconciled_plan(
        plan_path,
        project_root,
        &live_content,
        &extracted,
        &live_metadata,
    )
    .map(|()| true)
    .map_err(|()| "failed to write reconciled plan")
}

/// Log why `reconcile_amended_field` skipped without writing, naming the
/// plan, the stage, and the reason.
pub(super) fn warn_skip(plan_path: &Path, stage_id: &str, reason: &str) {
    tracing::warn!(
        target: "loom::adjudication",
        plan = %plan_path.display(),
        stage = %stage_id,
        reason,
        "plan-versions catch-up: skipping Case 2 field reconciliation",
    );
}

/// Parse the live plan's metadata block, keeping the extracted fence range
/// (needed to splice a correction back in) alongside the parsed metadata.
fn parse_live_plan(content: &str) -> Result<(ExtractedMetadata, LoomMetadata), &'static str> {
    let extracted = extract_yaml_metadata_with_ranges(content)
        .map_err(|_| "live plan metadata is not parseable")?;
    let metadata = parse_and_validate(&extracted.yaml)
        .map_err(|_| "live plan metadata failed schema validation")?;
    Ok((extracted, metadata))
}

/// Parse `content`'s metadata and return the stage definition matching
/// `stage_id`, or `None` if either step fails.
fn find_stage_def(content: &str, stage_id: &str) -> Option<StageDefinition> {
    let extracted = extract_yaml_metadata_with_ranges(content).ok()?;
    let metadata = parse_and_validate(&extracted.yaml).ok()?;
    metadata.loom.stages.into_iter().find(|s| s.id == stage_id)
}

/// True when `field` holds the same value on both stage definitions.
/// `WiringCheck`/`WiringTest` don't derive `PartialEq`, so wiring fields are
/// compared by serialised form — the same approach `stage_field_matches`
/// (in the sibling `amendment_fields` module) uses for `Stage` vs
/// `StageDefinition`.
fn stage_def_field_matches(
    live: &StageDefinition,
    snap: &StageDefinition,
    field: AmendmentField,
) -> bool {
    match field {
        AmendmentField::Acceptance => live.acceptance == snap.acceptance,
        AmendmentField::Wiring => {
            serde_yaml::to_string(&live.wiring).unwrap_or_default()
                == serde_yaml::to_string(&snap.wiring).unwrap_or_default()
        }
        AmendmentField::WiringTests => {
            serde_yaml::to_string(&live.wiring_tests).unwrap_or_default()
                == serde_yaml::to_string(&snap.wiring_tests).unwrap_or_default()
        }
    }
}

/// Set only `field` on `dest` from `src`, leaving every other field alone.
fn copy_field(dest: &mut StageDefinition, src: &StageDefinition, field: AmendmentField) {
    match field {
        AmendmentField::Acceptance => dest.acceptance = src.acceptance.clone(),
        AmendmentField::Wiring => dest.wiring = src.wiring.clone(),
        AmendmentField::WiringTests => dest.wiring_tests = src.wiring_tests.clone(),
    }
}

/// Re-serialise `metadata` and splice it into `live_content` in place of the
/// original metadata body, then write it back atomically.
fn write_reconciled_plan(
    plan_path: &Path,
    project_root: &Path,
    live_content: &str,
    extracted: &ExtractedMetadata,
    metadata: &LoomMetadata,
) -> Result<(), ()> {
    let new_yaml_body = serialize_loom_metadata(metadata).map_err(|_| ())?;
    let new_content = splice_metadata_yaml(live_content, extracted, &new_yaml_body);
    safe_fs::safe_replace_outside_workdir(plan_path, project_root, new_content.as_bytes())
        .map_err(|_| ())
}
