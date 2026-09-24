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
//! those of a criterion dispute; `verdict_kinds` holds the findings,
//! contract and integrity rules, built from the same parts.

use serde::Deserialize;
use serde_json::Value;

use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, PlanPatch};

use super::{plan_patch, verdict_kinds};

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

/// Parse `raw` as the verdict on a criterion dispute and either return a
/// usable verdict (possibly coerced) or signal that the stage must be
/// escalated to human review. Only a `needs-more-evidence` verdict can
/// escalate, under the same rule for every kind, so the relay's pre-check in
/// `loom stage adjudicate` uses this whatever the dispute's kind.
pub fn parse_and_validate(raw: &str) -> ValidationOutcome {
    match parse_json_object(raw) {
        Ok(json) => classify_and_validate(json),
        Err(coerced) => coerced,
    }
}

/// [`parse_and_validate`] for a dispute of `kind`.
pub fn parse_and_validate_for(raw: &str, kind: &DisputeKind) -> ValidationOutcome {
    match (parse_json_object(raw), kind) {
        (Ok(json), DisputeKind::Criterion { .. }) => classify_and_validate(json),
        (Ok(json), _) => verdict_kinds::classify_for_kind(&json, kind),
        (Err(coerced), _) => coerced,
    }
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

fn classify_and_validate(json: Value) -> ValidationOutcome {
    let Some(normalized) = verdict_tag(&json) else {
        return needs_more_evidence(
            "Adjudicator output missing required 'verdict' field. Must be one of: accept, reject, needs-more-evidence.",
        );
    };
    match normalized.as_str() {
        "accept" => validate_accept(&json),
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
    let reasoning = match json.get("reasoning").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            return Err(needs_more_evidence(format!(
                "{verdict} verdict missing non-empty 'reasoning' string."
            )));
        }
    };
    let citations = parse_citations(json.get("citations")).map_err(needs_more_evidence)?;
    if citations.is_empty() {
        return Err(needs_more_evidence(format!(
            "{verdict} verdict must include at least one citation grounding the decision."
        )));
    }
    Ok((reasoning, citations))
}

fn validate_accept(json: &Value) -> ValidationOutcome {
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
    let (field, patch, patch_reason) = match plan_patch::normalize(&plan_patch_raw) {
        Ok(parts) => parts,
        Err(msg) => {
            return needs_more_evidence(format!(
                "Accept verdict 'plan_patch' is malformed: {msg}. Re-emit it as: {{\"field\": \
                 \"acceptance\"|\"wiring\", \"patch\": {{\"op\": \"replace\"|\"insert\"|\"delete\", \
                 \"index\": <0-based int>, \"value\": \"<YAML body; omit for delete>\"}}, \"reason\": \
                 \"<why the criterion is wrong>\"}}"
            ));
        }
    };
    ValidationOutcome::Verdict(DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: plan_patch::canonical_inner(field, &patch, patch_reason.as_deref()),
        },
        citations,
        reasoning,
    })
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
