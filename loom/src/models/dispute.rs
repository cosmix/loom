//! On-disk types for the autonomous-criteria-adjudication subsystem.
//!
//! Trust boundary: agents (or the daemon RPC handler acting on their behalf)
//! write `request.md`; the daemon — and ONLY the daemon — writes
//! `verdict.md` and the zero-byte `applied.marker`. Layout helpers at the
//! bottom of this module encode the on-disk shape.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request to dispute a stage's acceptance criterion. Written by the
/// agent (or on its behalf by the daemon RPC handler) to
/// `.work/disputes/<stage>/<n>/request.md`. The agent attests to the
/// failure; the adjudicator returns a separate verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisputeRequest {
    pub id: u32,
    pub stage_id: String,
    pub criterion_index: usize,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_output: Option<String>,
    pub fix_attempts_at_dispute: u32,
    pub created_at: DateTime<Utc>,
}

/// A citation grounds a verdict in concrete code. The structural
/// requirement (file + claim) is the proxy for confidence — there is
/// no separate `confidence` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Citation {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub excerpt: String,
    pub claim: String,
}

/// Forward-declaration of the typed plan amendment shape that Stage 3
/// will introduce. The placeholder accepts an opaque JSON object so
/// the schema can be tightened in Stage 3 without breaking Stage 2.
///
/// Stage 3 will replace this with structured `AmendmentField` + `AmendmentPatch`
/// types. Stage 2 only persists the JSON so deserialisation remains
/// stable across the transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanPatch {
    /// Opaque JSON until Stage 3 lands the typed amendment schema.
    #[serde(flatten)]
    pub inner: serde_json::Value,
}

/// The adjudicator's verdict on a DisputeRequest. There is intentionally
/// no `NeedsHumanReview` variant — escalations transition the *stage*
/// to NeedsHumanReview directly without writing a verdict file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum DisputeVerdict {
    Accept {
        plan_patch: PlanPatch,
        citations: Vec<Citation>,
        reasoning: String,
    },
    Reject {
        citations: Vec<Citation>,
        reasoning: String,
    },
    NeedsMoreEvidence {
        questions: Vec<String>,
    },
}

/// Verdict record written to `.work/disputes/<stage>/<n>/verdict.md` by
/// the daemon (NEVER by an agent). Apply state is signalled by the
/// existence of a sibling `applied.marker` zero-byte file (also written
/// only by the daemon).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisputeVerdictRecord {
    pub id: u32,
    pub stage_id: String,
    pub verdict: DisputeVerdict,
    pub adjudicator_attempt_count: u32,
    pub created_at: DateTime<Utc>,
    pub model: String,
}

/// Layout helpers for the on-disk dispute directory:
/// `.work/disputes/<stage>/<n>/{request.md,verdict.md,applied.marker}`.
pub fn dispute_dir(disputes_root: &std::path::Path, stage_id: &str, id: u32) -> PathBuf {
    disputes_root.join(stage_id).join(id.to_string())
}

pub fn request_file(disputes_root: &std::path::Path, stage_id: &str, id: u32) -> PathBuf {
    dispute_dir(disputes_root, stage_id, id).join("request.md")
}

pub fn verdict_file(disputes_root: &std::path::Path, stage_id: &str, id: u32) -> PathBuf {
    dispute_dir(disputes_root, stage_id, id).join("verdict.md")
}

pub fn applied_marker(disputes_root: &std::path::Path, stage_id: &str, id: u32) -> PathBuf {
    dispute_dir(disputes_root, stage_id, id).join("applied.marker")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispute_request_round_trip_yaml() {
        let req = DisputeRequest {
            id: 1,
            stage_id: "stage-a".to_string(),
            criterion_index: 2,
            reason: "criterion impossible".to_string(),
            evidence_commit: Some("abc123".to_string()),
            failure_output: Some("error: ...".to_string()),
            fix_attempts_at_dispute: 1,
            created_at: Utc::now(),
        };
        let y = serde_yaml::to_string(&req).unwrap();
        let back: DisputeRequest = serde_yaml::from_str(&y).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn verdict_accept_serializes_with_citations() {
        let v = DisputeVerdictRecord {
            id: 1,
            stage_id: "stage-a".to_string(),
            verdict: DisputeVerdict::Accept {
                plan_patch: PlanPatch {
                    inner: serde_json::json!({"foo": "bar"}),
                },
                citations: vec![Citation {
                    file: "src/foo.rs".to_string(),
                    line: Some(42),
                    excerpt: "fn foo()".to_string(),
                    claim: "function exists".to_string(),
                }],
                reasoning: "evidence supports".to_string(),
            },
            adjudicator_attempt_count: 1,
            created_at: Utc::now(),
            model: "claude-sonnet".to_string(),
        };
        let y = serde_yaml::to_string(&v).unwrap();
        let back: DisputeVerdictRecord = serde_yaml::from_str(&y).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn verdict_reject_no_plan_patch_field() {
        let v = DisputeVerdict::Reject {
            citations: vec![],
            reasoning: "no evidence".to_string(),
        };
        let s = serde_yaml::to_string(&v).unwrap();
        assert!(s.contains("reject"), "verdict tag missing: {s}");
        assert!(
            !s.contains("plan_patch"),
            "Reject must not serialize plan_patch: {s}"
        );
    }

    #[test]
    fn verdict_needs_more_evidence_carries_questions() {
        let v = DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["clarify A".to_string(), "clarify B".to_string()],
        };
        let s = serde_yaml::to_string(&v).unwrap();
        assert!(s.contains("needs-more-evidence"), "wrong tag: {s}");
        assert!(s.contains("clarify A"));
    }
}
