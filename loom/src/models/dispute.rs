//! On-disk types for the autonomous-criteria-adjudication subsystem.
//!
//! Trust boundary: agents (or the daemon RPC handler acting on their behalf)
//! write `request.md`; the daemon — and ONLY the daemon — writes
//! `verdict.md` and the zero-byte `applied.marker`. Layout helpers at the
//! bottom of this module encode the on-disk shape.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::verify::integrity::IntegrityEvent;
use crate::verify::review::report::Finding;
use crate::verify::review::store::{OpenFinding, RulingKind};

/// A disputed review finding as it stood open when the dispute was filed:
/// `origin_stage` is set for a finding carried from another stage, `round` is
/// the review round that raised it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingSnapshot {
    pub id: String,
    pub origin_stage: Option<String>,
    pub round: u32,
    pub finding: Finding,
}

/// A disputed test-integrity event as it stood when the dispute was filed.
pub type IntegritySnapshot = IntegrityEvent;

/// What a dispute contests (DESIGN D15). `request.md` carries it as a `kind`
/// key beside the kind's own fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DisputeKind {
    /// One acceptance criterion: `loom stage dispute-criteria`.
    Criterion { criterion_index: usize },
    /// Open review findings, own or carried: `loom stage dispute-findings`.
    Findings {
        finding_ids: Vec<String>,
        evidence: Vec<FindingSnapshot>,
    },
    /// One frozen contract: `loom stage dispute-contract`.
    Contract { contract_id: String },
    /// Current test-integrity events: `loom stage dispute-integrity`.
    Integrity {
        event_ids: Vec<String>,
        evidence: Vec<IntegritySnapshot>,
    },
}

impl DisputeKind {
    /// The kind as `request.md` names it.
    pub fn name(&self) -> &'static str {
        match self {
            DisputeKind::Criterion { .. } => "criterion",
            DisputeKind::Findings { .. } => "findings",
            DisputeKind::Contract { .. } => "contract",
            DisputeKind::Integrity { .. } => "integrity",
        }
    }
}

/// Snapshots of the findings `ids` names among the stage's `open` findings, in
/// the order named. Fails on an empty list, on an id named twice, and on the
/// first id that is not open.
pub fn select_findings(open: &[OpenFinding], ids: &[String]) -> Result<Vec<FindingSnapshot>> {
    select(ids, "open review finding", |id| {
        open.iter()
            .find(|finding| finding.id == id)
            .map(snapshot_finding)
    })
}

/// The events `ids` names among the stage's current integrity `events`, under
/// the same rules as [`select_findings`].
pub fn select_events(events: &[IntegrityEvent], ids: &[String]) -> Result<Vec<IntegritySnapshot>> {
    select(ids, "current test-integrity event", |id| {
        events
            .iter()
            .find(|event| event.id == id)
            .map(|event| Ok(event.clone()))
    })
}

fn select<T>(
    ids: &[String],
    what: &str,
    find: impl Fn(&str) -> Option<Result<T>>,
) -> Result<Vec<T>> {
    if ids.is_empty() {
        bail!("no {what} is named");
    }
    let mut named = HashSet::new();
    ids.iter()
        .map(|id| {
            if !named.insert(id.as_str()) {
                bail!("'{id}' is named twice");
            }
            find(id.as_str())
                .unwrap_or_else(|| Err(anyhow!("'{id}' names no {what} of this stage")))
        })
        .collect()
}

fn snapshot_finding(open: &OpenFinding) -> Result<FindingSnapshot> {
    let round = review_round(&open.id)
        .with_context(|| format!("finding id '{}' names no review round", open.id))?;
    Ok(FindingSnapshot {
        id: open.id.clone(),
        origin_stage: open.origin_stage.clone(),
        round,
        finding: open.finding.clone(),
    })
}

/// The round of `F-<round>-<k>`, or of a carried `<origin-stage>/F-<round>-<k>`.
fn review_round(id: &str) -> Option<u32> {
    let local = id.rsplit('/').next()?;
    let (round, _) = local.strip_prefix("F-")?.split_once('-')?;
    round.parse().ok()
}

/// Request to dispute part of a stage's verification. Written by the daemon
/// on the agent's behalf to `.loom/work/disputes/<stage>/<n>/request.md`. The
/// agent attests to the failure; the adjudicator returns a separate verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisputeRequest {
    pub id: u32,
    pub stage_id: String,
    #[serde(flatten)]
    pub kind: DisputeKind,
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

/// The adjudicator's ruling on one disputed finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingRuling {
    pub finding: String,
    pub ruling: RulingKind,
    #[serde(default)]
    pub target_stage: Option<String>,
    pub reasoning: String,
    #[serde(default)]
    pub citations: Vec<Citation>,
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
    /// A findings dispute's verdict: one ruling per disputed finding.
    Rulings {
        rulings: Vec<FindingRuling>,
    },
}

/// Verdict record written to `.loom/work/disputes/<stage>/<n>/verdict.md` by
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
    /// The adjudication session that recorded the verdict, from
    /// `LOOM_SESSION_ID` in the judge's environment; the daemon retires that
    /// session once the verdict is applied. Absent on records written before
    /// this field existed.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Layout helpers for the on-disk dispute directory:
/// `.loom/work/disputes/<stage>/<n>/{request.md,verdict.md,applied.marker}`.
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
#[path = "dispute_tests.rs"]
mod tests;
