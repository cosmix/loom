//! Parse, validate, and coerce LLM-emitted JSON verdicts into the
//! strongly-typed [`DisputeVerdict`].
//!
//! The contract this module enforces is the bridge between an
//! unreliable string-shaped LLM output and the strict on-disk verdict
//! record consumed by [`crate::orchestrator::adjudication::mod`].
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

use serde::Deserialize;
use serde_json::Value;

use crate::models::dispute::{Citation, DisputeVerdict, PlanPatch};

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

/// Parse `raw` and either return a usable verdict (possibly coerced) or
/// signal that the stage must be escalated to human review.
pub fn parse_and_validate(raw: &str) -> ValidationOutcome {
    let json: Value = match parse_json_lenient(raw) {
        Ok(v) => v,
        Err(e) => {
            return ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence {
                questions: vec![format!(
                    "Adjudicator output was not valid JSON: {e}. Re-emit a single JSON object matching the schema."
                )],
            });
        }
    };
    classify_and_validate(json)
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
    let inner = s.strip_prefix("```json").or_else(|| s.strip_prefix("```"))?;
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
    let verdict_tag = match json.get("verdict").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => {
            return needs_more_evidence(
                "Adjudicator output missing required 'verdict' field. Must be one of: accept, reject, needs-more-evidence.",
            );
        }
    };
    let normalized = verdict_tag.to_lowercase().replace('_', "-");
    match normalized.as_str() {
        "accept" => validate_accept(&json),
        "reject" => validate_reject(&json),
        "needs-more-evidence" | "needs-evidence" => validate_needs_more(&json),
        other => needs_more_evidence(format!(
            "Adjudicator emitted unknown verdict tag '{other}'. Must be accept|reject|needs-more-evidence."
        )),
    }
}

fn validate_accept(json: &Value) -> ValidationOutcome {
    let reasoning = match json.get("reasoning").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            return needs_more_evidence("Accept verdict missing non-empty 'reasoning' string.");
        }
    };
    let citations = match parse_citations(json.get("citations")) {
        Ok(c) => c,
        Err(e) => return needs_more_evidence(e),
    };
    if citations.is_empty() {
        return needs_more_evidence(
            "Accept verdict must include at least one citation grounding the decision.",
        );
    }
    let plan_patch = match json.get("plan_patch") {
        Some(v) if !v.is_null() => v.clone(),
        _ => {
            return needs_more_evidence(
                "Accept verdict requires a 'plan_patch' object (AmendmentRequest shape).",
            );
        }
    };
    ValidationOutcome::Verdict(DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: plan_patch },
        citations,
        reasoning,
    })
}

fn validate_reject(json: &Value) -> ValidationOutcome {
    let reasoning = match json.get("reasoning").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            return needs_more_evidence("Reject verdict missing non-empty 'reasoning' string.");
        }
    };
    let citations = match parse_citations(json.get("citations")) {
        Ok(c) => c,
        Err(e) => return needs_more_evidence(e),
    };
    if citations.is_empty() {
        return needs_more_evidence(
            "Reject verdict must include at least one citation grounding the decision.",
        );
    }
    ValidationOutcome::Verdict(DisputeVerdict::Reject {
        citations,
        reasoning,
    })
}

fn validate_needs_more(json: &Value) -> ValidationOutcome {
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

fn parse_citations(v: Option<&Value>) -> Result<Vec<Citation>, String> {
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

fn needs_more_evidence(reason: impl Into<String>) -> ValidationOutcome {
    ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence {
        questions: vec![reason.into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_with_citations_round_trips() {
        let raw = r#"{
            "verdict": "accept",
            "reasoning": "criterion was unreachable",
            "citations": [
                {"file": "src/a.rs", "line": 10, "excerpt": "fn foo()", "claim": "missing function"}
            ],
            "plan_patch": {"stage_id": "x", "field": "acceptance", "patch": {"op": "delete", "index": 1}}
        }"#;
        let out = parse_and_validate(raw);
        match out {
            ValidationOutcome::Verdict(DisputeVerdict::Accept {
                citations,
                reasoning,
                ..
            }) => {
                assert_eq!(citations.len(), 1);
                assert_eq!(reasoning, "criterion was unreachable");
            }
            other => panic!("expected Accept, got {other:?}"),
        }
    }

    #[test]
    fn accept_without_citations_coerced_to_needs_more() {
        let raw = r#"{
            "verdict": "accept",
            "reasoning": "feels right",
            "citations": [],
            "plan_patch": {}
        }"#;
        match parse_and_validate(raw) {
            ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
                assert!(questions[0].contains("citation"));
            }
            other => panic!("expected NeedsMoreEvidence, got {other:?}"),
        }
    }

    #[test]
    fn reject_with_citations() {
        let raw = r#"{
            "verdict": "reject",
            "reasoning": "criterion is right",
            "citations": [{"file":"a","excerpt":"e","claim":"c"}]
        }"#;
        match parse_and_validate(raw) {
            ValidationOutcome::Verdict(DisputeVerdict::Reject { citations, .. }) => {
                assert_eq!(citations.len(), 1);
                assert!(citations[0].line.is_none());
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn needs_more_evidence_with_questions() {
        let raw = r#"{"verdict": "needs-more-evidence", "questions": ["what is X?"]}"#;
        match parse_and_validate(raw) {
            ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
                assert_eq!(questions, vec!["what is X?".to_string()]);
            }
            other => panic!("expected NeedsMoreEvidence, got {other:?}"),
        }
    }

    #[test]
    fn needs_more_evidence_with_no_questions_escalates() {
        let raw = r#"{"verdict": "needs-more-evidence", "questions": []}"#;
        match parse_and_validate(raw) {
            ValidationOutcome::Escalate { reason } => {
                assert!(reason.contains("pathological"));
            }
            other => panic!("expected Escalate, got {other:?}"),
        }
    }

    #[test]
    fn unknown_verdict_tag_coerces_to_needs_more() {
        let raw = r#"{"verdict": "bogus"}"#;
        match parse_and_validate(raw) {
            ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
                assert!(questions[0].contains("unknown verdict tag"));
            }
            other => panic!("expected NeedsMoreEvidence, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_coerces_to_needs_more() {
        let raw = "not json at all";
        match parse_and_validate(raw) {
            ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
                assert!(
                    questions[0].contains("not valid JSON"),
                    "got: {:?}",
                    questions
                );
            }
            other => panic!("expected NeedsMoreEvidence, got {other:?}"),
        }
    }

    #[test]
    fn fenced_json_is_accepted() {
        let raw = "```json\n{\"verdict\":\"reject\",\"reasoning\":\"r\",\"citations\":[{\"file\":\"f\",\"excerpt\":\"e\",\"claim\":\"c\"}]}\n```";
        assert!(matches!(
            parse_and_validate(raw),
            ValidationOutcome::Verdict(DisputeVerdict::Reject { .. })
        ));
    }

    #[test]
    fn json_with_leading_prose_extracts_first_object() {
        let raw = "Sure! Here is the verdict:\n{\"verdict\":\"reject\",\"reasoning\":\"r\",\"citations\":[{\"file\":\"f\",\"excerpt\":\"e\",\"claim\":\"c\"}]}\nLet me know if you want more.";
        assert!(matches!(
            parse_and_validate(raw),
            ValidationOutcome::Verdict(DisputeVerdict::Reject { .. })
        ));
    }
}
