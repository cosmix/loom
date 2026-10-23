//! Parse, validate, and coerce LLM-emitted JSON verdicts into the
//! strongly-typed [`DisputeVerdict`].
//!
//! The contract this module enforces is the bridge between an
//! unreliable string-shaped LLM output and the strict on-disk verdict
//! record consumed by the [`crate::orchestrator::adjudication`] module.
//! Robustness rules:
//!
//! - Unparseable JSON → coerce to `NeedsMoreEvidence` with one question
//!   naming the parse error (so the next round prompts the agent for
//!   the schema we wanted).
//! - Shape mismatch (missing/wrong type on a required field) → coerce
//!   to `NeedsMoreEvidence` with a question naming the violation.
//! - Empty citations on Accept/Reject → coerce to `NeedsMoreEvidence`
//!   (Accept/Reject must be grounded).
//! - Empty questions on NeedsMoreEvidence → escalate via
//!   [`ValidationOutcome::Escalate`] — pathological LLM output that
//!   would loop forever if re-prompted.
//!
//! Which verdicts a dispute takes depends on its kind: the rules above are
//! those of a criterion dispute, whose accept must also amend a list the
//! disputed one admits (see `admits`); `verdict_kinds` holds the findings,
//! contract and integrity rules, built from the same parts. A freeze-refusal
//! verdict is checked against its stage and worktree instead, and refused
//! rather than coerced (`parse_and_validate_in`, `verdict_freeze_refusal`).

use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

use crate::models::dispute::{Citation, CriterionField, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::Stage;
use crate::plan::amendment::AmendmentField;

use super::{plan_patch, verdict_freeze_refusal, verdict_kinds};

/// Result of parsing + validating raw JSON from the model.
///
/// `Verdict` is the normal happy path. `Escalate` signals that the
/// adjudicator emitted something so degenerate (e.g. NeedsMoreEvidence
/// with no questions) that we should bypass the verdict-file path and
/// transition the stage directly to `NeedsHumanReview`.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationOutcome {
    Verdict(DisputeVerdict),
    Escalate { reason: String },
}

/// Parse `raw` as the verdict on an acceptance-criterion dispute and either
/// return a usable verdict (possibly coerced) or signal that the stage must be
/// escalated to human review. Only a `needs-more-evidence` verdict can
/// escalate, under the same rule for every kind, so the relay's pre-check in
/// `loom stage adjudicate` uses this whatever the dispute's kind; the verdict
/// it records goes through [`parse_and_validate_for`].
pub fn parse_and_validate(raw: &str) -> ValidationOutcome {
    match parse_json_object(raw) {
        Ok(json) => classify_and_validate(json, CriterionField::Acceptance),
        Err(coerced) => coerced,
    }
}

/// [`parse_and_validate`] for a dispute of `kind`.
pub fn parse_and_validate_for(raw: &str, kind: &DisputeKind) -> ValidationOutcome {
    match (parse_json_object(raw), kind) {
        (Ok(json), DisputeKind::Criterion { field, .. }) => classify_and_validate(json, *field),
        (Ok(json), _) => verdict_kinds::classify_for_kind(&json, kind),
        (Err(coerced), _) => coerced,
    }
}

/// What a verdict is recorded against: the state directory and the disputed
/// stage, as `record.rs` loaded it.
pub(crate) struct RecordContext<'a> {
    pub work_dir: &'a Path,
    pub stage: &'a Stage,
}

/// [`parse_and_validate_for`] at recording, where the stage and its worktree
/// are at hand. A freeze-refusal verdict is checked against both, and every
/// defect is an `Err` naming it for the judge to correct: coerced to
/// needs-more-evidence, it would reach the contract writer and cost it a
/// filing. Every other kind keeps its self-correcting rules.
pub(crate) fn parse_and_validate_in(
    raw: &str,
    kind: &DisputeKind,
    context: &RecordContext<'_>,
) -> Result<ValidationOutcome, String> {
    if !matches!(kind, DisputeKind::FreezeRefusal { .. }) {
        return Ok(parse_and_validate_for(raw, kind));
    }
    let json = parse_json_lenient(raw)
        .map_err(|e| format!("the verdict is not valid JSON: {e}; write a single JSON object"))?;
    verdict_freeze_refusal::validate(&json, context).map(ValidationOutcome::Verdict)
}

/// The JSON in `raw`, or the question asking for it again.
fn parse_json_object(raw: &str) -> Result<Value, ValidationOutcome> {
    parse_json_lenient(raw).map_err(|e| {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence {
            questions: vec![format!(
                "Adjudicator output was not valid JSON: {e}. Re-emit a single JSON object matching the schema."
            )],
        })
    })
}

/// Permissive JSON parser: tries the input verbatim, then strips a
/// surrounding ```json``` fence if present, then takes the first
/// top-level `{...}` substring.
fn parse_json_lenient(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return Ok(v);
    }
    let trimmed = raw.trim();
    if let Some(stripped) = strip_code_fence(trimmed) {
        if let Ok(v) = serde_json::from_str::<Value>(stripped) {
            return Ok(v);
        }
    }
    if let Some(brace_slice) = extract_first_object(trimmed) {
        if let Ok(v) = serde_json::from_str::<Value>(brace_slice) {
            return Ok(v);
        }
    }
    // Trigger the canonical "not JSON" error path so the caller sees a
    // representative message.
    serde_json::from_str::<Value>(raw)
}

fn strip_code_fence(s: &str) -> Option<&str> {
    let inner = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))?;
    let body = inner.trim_start_matches('\n');
    let end = body.rfind("```")?;
    Some(body[..end].trim())
}

fn extract_first_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    // Bracket-matching pass that respects strings + escapes.
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    end.map(|e| &s[start..e])
}

/// A criterion dispute's verdict; `disputed` is the list the dispute contests.
fn classify_and_validate(json: Value, disputed: CriterionField) -> ValidationOutcome {
    let Some(normalized) = verdict_tag(&json) else {
        return needs_more_evidence(
            "Adjudicator output missing required 'verdict' field. Must be one of: accept, reject, needs-more-evidence.",
        );
    };
    match normalized.as_str() {
        "accept" => validate_accept(&json, disputed),
        "reject" => validate_reject(&json),
        "needs-more-evidence" | "needs-evidence" => validate_needs_more(&json),
        other => needs_more_evidence(format!(
            "Adjudicator emitted unknown verdict tag '{other}'. Must be accept|reject|needs-more-evidence."
        )),
    }
}

/// The `verdict` tag, lower-cased with `_` read as `-`.
pub(super) fn verdict_tag(json: &Value) -> Option<String> {
    let tag = json.get("verdict").and_then(|v| v.as_str())?;
    Some(tag.to_lowercase().replace('_', "-"))
}

/// The non-empty `reasoning` and the citations, at least one, that an
/// `accept` or `reject` (named by `verdict`) must be grounded in.
pub(super) fn grounding(
    json: &Value,
    verdict: &str,
) -> Result<(String, Vec<Citation>), ValidationOutcome> {
    grounding_text(json, verdict).map_err(needs_more_evidence)
}

/// [`grounding`], its defect as the message alone.
pub(super) fn grounding_text(
    json: &Value,
    verdict: &str,
) -> Result<(String, Vec<Citation>), String> {
    let reasoning = match json.get("reasoning").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            return Err(format!(
                "{verdict} verdict missing non-empty 'reasoning' string."
            ))
        }
    };
    let citations = parse_citations(json.get("citations"))?;
    if citations.is_empty() {
        return Err(format!(
            "{verdict} verdict must include at least one citation grounding the decision."
        ));
    }
    Ok((reasoning, citations))
}

fn validate_accept(json: &Value, disputed: CriterionField) -> ValidationOutcome {
    let (reasoning, citations) = match grounding(json, "Accept") {
        Ok(grounded) => grounded,
        Err(coerced) => return coerced,
    };
    let plan_patch_raw = match json.get("plan_patch") {
        Some(v) if !v.is_null() => v.clone(),
        _ => {
            return needs_more_evidence(
                "Accept verdict requires a 'plan_patch' object (AmendmentRequest shape).",
            );
        }
    };
    let (field, patches, patch_reason) = match plan_patch::normalize(&plan_patch_raw) {
        Ok(parts) => parts,
        Err(msg) => {
            return needs_more_evidence(format!(
                "Accept verdict 'plan_patch' is malformed: {msg}. Re-emit it as: {{\"field\": \
                 {}, \"patches\": [{{\"op\": \"replace\"|\"insert\"|\"delete\", \
                 \"index\": <0-based int>, \"value\": \"<YAML body; omit for delete>\"}}, ...], \
                 \"reason\": \"<why the criteria are wrong>\"}}",
                admitted_fields(disputed)
            ));
        }
    };
    if !admits(disputed, field) {
        return needs_more_evidence(wrong_list(disputed, &plan_patch_raw));
    }
    if let Some(index) = plan_patch::duplicate_index(&patches) {
        return needs_more_evidence(format!(
            "Accept verdict 'plan_patch' patches index {index} twice. Every index names the list \
             as filed; re-emit one patch per index."
        ));
    }
    ValidationOutcome::Verdict(DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: plan_patch::canonical_patches_inner(field, &patches, patch_reason.as_deref()),
        },
        citations,
        reasoning,
    })
}

/// Whether an accept on a dispute over `disputed`'s list may amend `field`:
/// an acceptance dispute may amend `acceptance` or `wiring`, a wiring or
/// wiring-tests dispute only its own list.
fn admits(disputed: CriterionField, field: AmendmentField) -> bool {
    field == disputed.amendment_field()
        || (disputed == CriterionField::Acceptance && field == AmendmentField::Wiring)
}

/// The `plan_patch.field` values an accept on a dispute over `disputed`'s
/// list may carry, as a re-prompt spells them.
fn admitted_fields(disputed: CriterionField) -> &'static str {
    match disputed {
        CriterionField::Acceptance => "\"acceptance\"|\"wiring\"",
        CriterionField::Wiring => "\"wiring\"",
        CriterionField::WiringTests => "\"wiring-tests\"",
    }
}

/// The question for an accept whose `plan_patch` amends a list the dispute
/// does not contest: it names the disputed list and the field to use.
fn wrong_list(disputed: CriterionField, plan_patch_raw: &Value) -> String {
    let amended = plan_patch_raw
        .get("field")
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!(
        "Accept verdict 'plan_patch' amends '{amended}', but this dispute contests the \
         stage's {} list. Re-emit plan_patch with \"field\": {}.",
        disputed.as_str(),
        admitted_fields(disputed)
    )
}

pub(super) fn validate_reject(json: &Value) -> ValidationOutcome {
    match grounding(json, "Reject") {
        Ok((reasoning, citations)) => ValidationOutcome::Verdict(DisputeVerdict::Reject {
            citations,
            reasoning,
        }),
        Err(coerced) => coerced,
    }
}

pub(super) fn validate_needs_more(json: &Value) -> ValidationOutcome {
    let arr = match json.get("questions").and_then(|v| v.as_array()) {
        Some(a) => a.clone(),
        None => {
            return ValidationOutcome::Escalate {
                reason:
                    "Adjudicator returned needs-more-evidence with no 'questions' array — pathological output, not self-correctable."
                        .to_string(),
            };
        }
    };
    let questions: Vec<String> = arr
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    if questions.is_empty() {
        return ValidationOutcome::Escalate {
            reason:
                "Adjudicator returned needs-more-evidence with empty questions list — pathological output."
                    .to_string(),
        };
    }
    ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions })
}

pub(super) fn parse_citations(v: Option<&Value>) -> Result<Vec<Citation>, String> {
    let Some(v) = v else {
        return Err("missing 'citations' array".to_string());
    };
    let arr = v
        .as_array()
        .ok_or_else(|| "'citations' must be a JSON array".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        #[derive(Deserialize)]
        struct RawCitation {
            file: String,
            line: Option<u32>,
            excerpt: String,
            claim: String,
        }
        let raw: RawCitation = serde_json::from_value(item.clone())
            .map_err(|e| format!("citation #{i} malformed: {e}"))?;
        if raw.file.trim().is_empty() {
            return Err(format!("citation #{i} has empty 'file'"));
        }
        if raw.excerpt.trim().is_empty() {
            return Err(format!("citation #{i} has empty 'excerpt'"));
        }
        if raw.claim.trim().is_empty() {
            return Err(format!("citation #{i} has empty 'claim'"));
        }
        out.push(Citation {
            file: raw.file,
            line: raw.line,
            excerpt: raw.excerpt,
            claim: raw.claim,
        });
    }
    Ok(out)
}

pub(super) fn needs_more_evidence(reason: impl Into<String>) -> ValidationOutcome {
    ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence {
        questions: vec![reason.into()],
    })
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
