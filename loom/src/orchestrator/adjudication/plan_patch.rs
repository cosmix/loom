//! Normalising an adjudicator's `plan_patch` into [`AmendmentRequest`]s.
//!
//! A criterion verdict's canonical `plan_patch` is
//! `{"field": ..., "patches": [{"op", "index", "value"}, ...], "reason": ...}`:
//! one dispute names every failing criterion, so one verdict may amend several
//! entries of one list. [`normalize`] also reads the single nested
//! `{"field", "patch": {...}}` shape and the FLAT `{"field", "op", "index",
//! "value"}` shape three adjudicator sessions emitted when the prompt named
//! the shape only as "AmendmentRequest JSON", a Rust type the model cannot
//! see; `verdict.rs` accepts all three at record time and records `patches`.
//!
//! A contract verdict keeps the single `patch` (see [`decode_patch`]).

use anyhow::Result;
use serde_json::{Map, Value};

use crate::models::dispute::PlanPatch;
use crate::plan::amendment::{AmendmentField, AmendmentPatch, AmendmentRequest};

/// The amendments a recorded criterion verdict's `plan_patch` makes, in the
/// order they apply. Every index names the list as it stood when the dispute
/// was filed, so the patches apply by DESCENDING index: a delete or insert
/// never shifts an entry a later patch names. Patch `k` of the recorded list
/// carries `dispute_id` `"<dispute>.<k>"`, which keeps `apply_amendment`
/// idempotent per patch when an interrupted apply runs again.
pub(super) fn build_amendment_requests(
    stage_id: &str,
    plan_patch: &PlanPatch,
    dispute_id: u32,
) -> Result<Vec<AmendmentRequest>> {
    let (field, patches, reason) =
        normalize(&plan_patch.inner).map_err(|msg| anyhow::anyhow!(msg))?;
    let mut requests: Vec<AmendmentRequest> = patches
        .into_iter()
        .enumerate()
        .map(|(k, patch)| AmendmentRequest {
            stage_id: stage_id.to_string(),
            field,
            patch,
            reason: reason.clone(),
            dispute_id: Some(format!("{dispute_id}.{k}")),
        })
        .collect();
    requests.sort_by_key(|request| std::cmp::Reverse(request.patch.index()));
    Ok(requests)
}

/// Decode `inner` into `(field, patches, reason)`, accepting a `patches`
/// array, a nested `patch` object or the flat shape. The wiring-tests field
/// is read in its kebab or snake spelling.
///
/// Returns a human-readable error STRING rather than `anyhow::Error`:
/// `verdict.rs` needs the message verbatim inside a re-prompt question, and
/// `apply.rs` wraps it with `anyhow::anyhow!` for its own error path.
pub(super) fn normalize(
    inner: &Value,
) -> Result<(AmendmentField, Vec<AmendmentPatch>, Option<String>), String> {
    let field = match inner.get("field").and_then(|v| v.as_str()) {
        Some("acceptance") => AmendmentField::Acceptance,
        Some("wiring") => AmendmentField::Wiring,
        Some("wiring-tests" | "wiring_tests") => AmendmentField::WiringTests,
        Some(other) => {
            return Err(format!(
                "plan_patch field '{other}' must be acceptance|wiring|wiring-tests"
            ))
        }
        None => return Err("plan_patch missing 'field' string".to_string()),
    };
    let Some(raw) = inner.get("patches") else {
        let (patch, reason) = decode_patch(inner)?;
        return Ok((field, vec![patch], reason));
    };
    let items = raw
        .as_array()
        .ok_or("plan_patch 'patches' must be an array of {op, index, value} objects")?;
    if items.is_empty() {
        return Err("plan_patch 'patches' is empty; name at least one {op, index, value}".into());
    }
    let patches = items
        .iter()
        .enumerate()
        .map(|(k, item)| {
            serde_json::from_value(item.clone()).map_err(|e| format!("patches[{k}] malformed: {e}"))
        })
        .collect::<Result<Vec<AmendmentPatch>, String>>()?;
    Ok((field, patches, reason_of(inner)))
}

/// The first index two of `patches` both name, if any.
pub(super) fn duplicate_index(patches: &[AmendmentPatch]) -> Option<usize> {
    let mut seen = std::collections::HashSet::new();
    patches
        .iter()
        .map(AmendmentPatch::index)
        .find(|index| !seen.insert(*index))
}

/// Decode the single patch and reason of `inner`, in the nested or flat
/// shape, leaving its `field` to the caller: a contract dispute amends
/// `contracts` with exactly one patch.
pub(super) fn decode_patch(inner: &Value) -> Result<(AmendmentPatch, Option<String>), String> {
    let patch: AmendmentPatch = if let Some(patch_obj) = inner.get("patch") {
        serde_json::from_value(patch_obj.clone())
            .map_err(|e| format!("nested 'patch' object malformed: {e}"))?
    } else if inner.get("op").and_then(|v| v.as_str()).is_some() {
        // Collect the sibling op/index/value keys into a fresh object and
        // decode THAT through the same AmendmentPatch deserializer, rather
        // than matching op names by hand — this keeps op-name handling and
        // error messages identical between the two shapes.
        let mut map = Map::new();
        for key in ["op", "index", "value"] {
            if let Some(value) = inner.get(key) {
                map.insert(key.to_string(), value.clone());
            }
        }
        serde_json::from_value(Value::Object(map))
            .map_err(|e| format!("flat 'op'/'index'/'value' shape malformed: {e}"))?
    } else {
        return Err(
            "plan_patch has neither a 'patches' array, a nested 'patch' object nor a top-level \
             'op'; expected {\"field\": \"acceptance\", \"patches\": [{\"op\": \"replace\", \
             \"index\": 0, \"value\": \"<YAML>\"}]}"
                .to_string(),
        );
    };
    Ok((patch, reason_of(inner)))
}

fn reason_of(inner: &Value) -> Option<String> {
    inner
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// The canonical criterion `plan_patch`: `{"field", "patches": [...],
/// "reason"}`, whichever shape the adjudicator emitted.
pub(super) fn canonical_patches_inner(
    field: AmendmentField,
    patches: &[AmendmentPatch],
    reason: Option<&str>,
) -> Value {
    let patches = serde_json::to_value(patches).expect("AmendmentPatch always serializes");
    canonical(field, "patches", patches, reason)
}

/// The canonical single-patch `plan_patch`, `{"field", "patch": {...},
/// "reason"}`, a contract verdict records.
pub(super) fn canonical_inner(
    field: AmendmentField,
    patch: &AmendmentPatch,
    reason: Option<&str>,
) -> Value {
    let patch = serde_json::to_value(patch).expect("AmendmentPatch always serializes");
    canonical(field, "patch", patch, reason)
}

fn canonical(field: AmendmentField, key: &str, patch: Value, reason: Option<&str>) -> Value {
    let mut map = Map::new();
    map.insert(
        "field".to_string(),
        serde_json::to_value(field).expect("AmendmentField always serializes"),
    );
    map.insert(key.to_string(), patch);
    if let Some(reason) = reason {
        map.insert("reason".to_string(), Value::String(reason.to_string()));
    }
    Value::Object(map)
}

#[cfg(test)]
#[path = "plan_patch_tests.rs"]
mod tests;
