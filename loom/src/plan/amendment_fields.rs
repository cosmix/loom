//! Field-dispatch helpers for [`super::amendment`].
//!
//! These functions translate an [`AmendmentField`] into the concrete
//! `acceptance` / `wiring` / `wiring_tests` array on a [`StageDefinition`] or
//! runtime [`Stage`], apply an [`AmendmentPatch`] to that array, and persist
//! an amended stage back to disk.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::models::stage::Stage;
use crate::plan::schema::StageDefinition;
use crate::verify::transitions::update_stage;

use super::amendment::{AmendmentField, AmendmentPatch, ParsedAmendmentValue};

pub(super) fn current_field_len(stage: &StageDefinition, field: AmendmentField) -> usize {
    match field {
        AmendmentField::Acceptance => stage.acceptance.len(),
        AmendmentField::Wiring => stage.wiring.len(),
        AmendmentField::WiringTests => stage.wiring_tests.len(),
    }
}

pub(super) fn apply_patch_to_stage_def(
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
        AmendmentField::WiringTests => apply_patch_vec(
            &mut stage.wiring_tests,
            patch,
            match value {
                ParsedAmendmentValue::WiringTest(v) => Some(v.clone()),
                _ => None,
            },
        ),
    }
}

pub(super) fn apply_patch_to_runtime_stage(
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
        AmendmentField::WiringTests => apply_patch_vec(
            &mut stage.wiring_tests,
            patch,
            match value {
                ParsedAmendmentValue::WiringTest(v) => Some(v.clone()),
                _ => None,
            },
        ),
    }
}

pub(super) fn apply_patch_vec<T: Clone>(
    vec: &mut Vec<T>,
    patch: &AmendmentPatch,
    new_value: Option<T>,
) -> Result<()> {
    match patch {
        AmendmentPatch::Replace { index, .. } => {
            if *index >= vec.len() {
                bail!("Replace index {} out of bounds (len {})", index, vec.len());
            }
            let v =
                new_value.ok_or_else(|| anyhow::anyhow!("Replace patch missing typed value"))?;
            vec[*index] = v;
        }
        AmendmentPatch::Insert { index, .. } => {
            if *index > vec.len() {
                bail!("Insert index {} out of bounds (len {})", index, vec.len());
            }
            let v = new_value.ok_or_else(|| anyhow::anyhow!("Insert patch missing typed value"))?;
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

pub(super) fn stage_field_matches(
    stage: &Stage,
    def: &StageDefinition,
    field: AmendmentField,
) -> bool {
    match field {
        AmendmentField::Acceptance => stage.acceptance == def.acceptance,
        AmendmentField::Wiring => {
            // WiringCheck doesn't derive PartialEq; compare by serialized form.
            let a = serde_yaml::to_string(&stage.wiring).unwrap_or_default();
            let b = serde_yaml::to_string(&def.wiring).unwrap_or_default();
            a == b
        }
        AmendmentField::WiringTests => {
            // WiringTest doesn't derive PartialEq; compare by serialized form.
            let a = serde_yaml::to_string(&stage.wiring_tests).unwrap_or_default();
            let b = serde_yaml::to_string(&def.wiring_tests).unwrap_or_default();
            a == b
        }
    }
}

pub(super) fn sync_stage_from_definition(
    stage: &mut Stage,
    def: &StageDefinition,
    field: AmendmentField,
) {
    match field {
        AmendmentField::Acceptance => {
            stage.acceptance = def.acceptance.clone();
        }
        AmendmentField::Wiring => {
            stage.wiring = def.wiring.clone();
        }
        AmendmentField::WiringTests => {
            stage.wiring_tests = def.wiring_tests.clone();
        }
    }
}

/// Persist the amended `acceptance`/`wiring`/`wiring_tests` onto the stage
/// file. Re-reads the on-disk stage under `update_stage`'s lock so a
/// concurrent dispute-thread / orchestrator write to other fields
/// (dispute_count, status, session, …) is not reverted (A-5). Without this,
/// the runtime keeps stale criteria via `sync_graph_with_stage_files`.
pub(super) fn persist_amended_stage(
    stage: &Stage,
    request_stage_id: &str,
    work_dir: &Path,
) -> Result<()> {
    let amended_acceptance = stage.acceptance.clone();
    let amended_wiring = stage.wiring.clone();
    let amended_wiring_tests = stage.wiring_tests.clone();
    update_stage(request_stage_id, work_dir, |s| {
        s.acceptance = amended_acceptance.clone();
        s.wiring = amended_wiring.clone();
        s.wiring_tests = amended_wiring_tests.clone();
        Ok(())
    })
    .with_context(|| format!("Failed to save amended stage '{request_stage_id}'"))?;
    Ok(())
}
