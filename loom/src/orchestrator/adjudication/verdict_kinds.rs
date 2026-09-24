//! Validating the verdict on a findings, contract or integrity dispute
//! (DESIGN D15), with the self-correcting rule `verdict.rs` applies to a
//! criterion dispute: a shape defect becomes `NeedsMoreEvidence` with a
//! question naming it.
//!
//! - findings: `rulings`, one per disputed finding id and none for another id,
//!   `defer` naming a `target_stage`; or `needs-more-evidence`. Whether the
//!   target may take the finding is checked when the verdict is applied, since
//!   stages move on meanwhile.
//! - contract and integrity: `accept`, `reject` or `needs-more-evidence`. A
//!   contract `accept` may carry a `plan_patch` on `contracts`; an empty
//!   `plan_patch` object stands for none.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, FindingRuling, PlanPatch};
use crate::plan::amendment::AmendmentField;
use crate::verify::review::store::RulingKind;

use super::plan_patch;
use super::verdict::{
    grounding, needs_more_evidence, parse_citations, validate_needs_more, validate_reject,
    verdict_tag, ValidationOutcome,
};

pub(super) fn classify_for_kind(json: &Value, kind: &DisputeKind) -> ValidationOutcome {
    let (name, allowed) = match kind {
        DisputeKind::Findings { .. } => ("findings", "rulings|needs-more-evidence"),
        DisputeKind::Contract { .. } => ("contract", "accept|reject|needs-more-evidence"),
        DisputeKind::Integrity { .. } => ("integrity", "accept|reject|needs-more-evidence"),
        DisputeKind::Criterion { .. } => ("criterion", "accept|reject|needs-more-evidence"),
    };
    let Some(tag) = verdict_tag(json) else {
        return needs_more_evidence(format!(
            "Adjudicator output missing required 'verdict' field. Must be one of: {allowed}."
        ));
    };
    match (kind, tag.as_str()) {
        (_, "needs-more-evidence" | "needs-evidence") => validate_needs_more(json),
        (DisputeKind::Findings { finding_ids, .. }, "rulings") => {
            validate_rulings(json, finding_ids)
        }
        (DisputeKind::Contract { .. } | DisputeKind::Integrity { .. }, "reject") => {
            validate_reject(json)
        }
        (DisputeKind::Contract { .. }, "accept") => validate_accept(json, contracts_patch(json)),
        (DisputeKind::Integrity { .. }, "accept") => validate_accept(json, Ok(no_patch())),
        (_, other) => needs_more_evidence(format!(
            "Adjudicator emitted verdict '{other}', which a {name} dispute does not take. \
             Must be {allowed}."
        )),
    }
}

/// An `accept` grounded like a criterion one, carrying `patch`.
fn validate_accept(json: &Value, patch: Result<Value, String>) -> ValidationOutcome {
    let (reasoning, citations) = match grounding(json, "Accept") {
        Ok(grounded) => grounded,
        Err(coerced) => return coerced,
    };
    match patch {
        Ok(inner) => ValidationOutcome::Verdict(DisputeVerdict::Accept {
            plan_patch: PlanPatch { inner },
            citations,
            reasoning,
        }),
        Err(msg) => needs_more_evidence(format!(
            "Accept verdict 'plan_patch' is malformed: {msg}. Omit it, or re-emit it as: \
             {{\"field\": \"contracts\", \"patch\": {{\"op\": \"replace\"|\"insert\"|\"delete\", \
             \"index\": <0-based int>, \"value\": \"<contract YAML; omit for delete>\"}}, \
             \"reason\": \"<why the contract is wrong>\"}}"
        )),
    }
}

/// A contract `accept`'s optional `plan_patch`, canonicalised to the nested
/// shape; absent or `null` is no patch.
fn contracts_patch(json: &Value) -> Result<Value, String> {
    let raw = match json.get("plan_patch") {
        None | Some(Value::Null) => return Ok(no_patch()),
        Some(raw) => raw,
    };
    match raw.get("field").and_then(Value::as_str) {
        Some("contracts") => {}
        Some(other) => return Err(format!("plan_patch field '{other}' must be contracts")),
        None => return Err("plan_patch missing 'field' string".to_string()),
    }
    let (patch, reason) = plan_patch::decode_patch(raw)?;
    Ok(plan_patch::canonical_inner(
        AmendmentField::Contracts,
        &patch,
        reason.as_deref(),
    ))
}

/// The `plan_patch` of an `accept` that amends nothing.
fn no_patch() -> Value {
    Value::Object(Map::new())
}

fn validate_rulings(json: &Value, finding_ids: &[String]) -> ValidationOutcome {
    let Some(items) = json.get("rulings").and_then(Value::as_array) else {
        return needs_more_evidence(
            "Rulings verdict requires a 'rulings' array with one ruling per disputed finding.",
        );
    };
    let parsed: Result<Vec<FindingRuling>, String> = items
        .iter()
        .enumerate()
        .map(|(i, item)| parse_ruling(i, item))
        .collect();
    match parsed.and_then(|rulings| check_coverage(rulings, finding_ids)) {
        Ok(rulings) => ValidationOutcome::Verdict(DisputeVerdict::Rulings { rulings }),
        Err(msg) => needs_more_evidence(msg),
    }
}

/// One ruling. `dismiss` and `defer` close a finding, so they must cite what
/// grounds them; `defer` must name its target stage.
fn parse_ruling(i: usize, item: &Value) -> Result<FindingRuling, String> {
    #[derive(Deserialize)]
    struct RawRuling {
        finding: String,
        ruling: RulingKind,
        #[serde(default)]
        target_stage: Option<String>,
        reasoning: String,
    }
    let raw: RawRuling =
        serde_json::from_value(item.clone()).map_err(|e| format!("ruling #{i} malformed: {e}"))?;
    let finding = raw.finding;
    if raw.reasoning.trim().is_empty() {
        return Err(format!(
            "ruling #{i} on '{finding}' has an empty 'reasoning'"
        ));
    }
    let citations: Vec<Citation> = match item.get("citations") {
        None | Some(Value::Null) => Vec::new(),
        present => parse_citations(present).map_err(|e| format!("ruling #{i}: {e}"))?,
    };
    if raw.ruling.closes() && citations.is_empty() {
        return Err(format!(
            "ruling #{i} closes '{finding}' without a citation grounding the decision"
        ));
    }
    let target_stage = match raw.ruling {
        RulingKind::Defer => Some(
            raw.target_stage
                .filter(|target| !target.trim().is_empty())
                .ok_or_else(|| {
                    format!("ruling #{i} defers '{finding}' without a 'target_stage'")
                })?,
        ),
        RulingKind::Uphold | RulingKind::Dismiss => None,
    };
    Ok(FindingRuling {
        finding,
        ruling: raw.ruling,
        target_stage,
        reasoning: raw.reasoning,
        citations,
    })
}

/// Exactly one ruling per disputed finding, and none for any other id.
fn check_coverage(
    rulings: Vec<FindingRuling>,
    finding_ids: &[String],
) -> Result<Vec<FindingRuling>, String> {
    if let Some(stray) = rulings.iter().find(|r| !finding_ids.contains(&r.finding)) {
        return Err(format!(
            "a ruling names '{}', which is not a disputed finding (disputed: {})",
            stray.finding,
            finding_ids.join(", ")
        ));
    }
    for id in finding_ids {
        let count = rulings.iter().filter(|r| r.finding == *id).count();
        if count != 1 {
            return Err(format!(
                "finding '{id}' has {count} rulings; give exactly one ruling per disputed finding"
            ));
        }
    }
    Ok(rulings)
}

#[cfg(test)]
#[path = "verdict_kinds_tests.rs"]
mod tests;
