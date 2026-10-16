//! Field-dispatch helpers for [`super::amendment`].
//!
//! These functions translate an [`AmendmentField`] into the concrete
//! `acceptance` / `wiring` / `wiring_tests` / `contracts` array on a
//! [`StageDefinition`] or runtime [`Stage`], apply an [`AmendmentPatch`] to
//! that array, and synchronize the amended runtime field.

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

use crate::models::stage::Stage;
use crate::plan::schema::StageDefinition;

use super::amendment::{AmendmentField, AmendmentPatch, AmendmentRequest, ParsedAmendmentValue};

pub(super) fn current_field_len(stage: &StageDefinition, field: AmendmentField) -> usize {
    match field {
        AmendmentField::Acceptance => stage.acceptance.len(),
        AmendmentField::Wiring => stage.wiring.len(),
        AmendmentField::WiringTests => stage.wiring_tests.len(),
        AmendmentField::Contracts => stage.contracts.len(),
    }
}

/// Deserialize a replace or insert `value` into the REAL type of the targeted
/// field, never a hand-rolled simplified shape, so a malformed patch fails
/// before anything is written. A delete carries no value.
pub(super) fn parse_amendment_value(request: &AmendmentRequest) -> Result<ParsedAmendmentValue> {
    let value = match &request.patch {
        AmendmentPatch::Replace { value, .. } | AmendmentPatch::Insert { value, .. } => value,
        AmendmentPatch::Delete { .. } => return Ok(ParsedAmendmentValue::None),
    };
    let stage = request.stage_id.as_str();
    Ok(match request.field {
        AmendmentField::Acceptance => {
            ParsedAmendmentValue::Acceptance(parse_yaml(value, "AcceptanceCriterion", stage)?)
        }
        AmendmentField::Wiring => {
            ParsedAmendmentValue::Wiring(parse_yaml(value, "WiringCheck", stage)?)
        }
        AmendmentField::WiringTests => {
            ParsedAmendmentValue::WiringTest(parse_yaml(value, "WiringTest", stage)?)
        }
        AmendmentField::Contracts => {
            ParsedAmendmentValue::Contract(parse_yaml(value, "ContractSpec", stage)?)
        }
    })
}

fn parse_yaml<T: DeserializeOwned>(value: &str, type_name: &str, stage_id: &str) -> Result<T> {
    crate::yaml::from_str(value)
        .with_context(|| format!("Invalid {type_name} in amendment for stage '{stage_id}'"))
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
        AmendmentField::Contracts => apply_patch_vec(
            &mut stage.contracts,
            patch,
            match value {
                ParsedAmendmentValue::Contract(v) => Some(v.clone()),
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
            let a = crate::yaml::to_string(&stage.wiring).unwrap_or_default();
            let b = crate::yaml::to_string(&def.wiring).unwrap_or_default();
            a == b
        }
        AmendmentField::WiringTests => {
            // WiringTest doesn't derive PartialEq; compare by serialized form.
            let a = crate::yaml::to_string(&stage.wiring_tests).unwrap_or_default();
            let b = crate::yaml::to_string(&def.wiring_tests).unwrap_or_default();
            a == b
        }
        AmendmentField::Contracts => stage.contracts == def.contracts,
    }
}

pub(super) fn sync_stage_from_definition(
    stage: &mut Stage,
    def: &StageDefinition,
    field: AmendmentField,
) {
    if !stage_field_matches(stage, def, field) {
        stage.completed_commit = None;
    }
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
        AmendmentField::Contracts => {
            stage.contracts = def.contracts.clone();
        }
    }
}
