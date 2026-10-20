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

use crate::plan::amendment::AmendmentField;
use crate::verify::impact_tests::ImpactFailure;
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

/// A disputed impact-selected test failure as loom's own gate run recorded it
/// (`reviews/<stage>/impact-observed.json`), base comparison included. The
/// daemon copies it from that record; the filing client sends only ids.
pub type ImpactSnapshot = ImpactFailure;

/// Which list of a stage a criterion dispute contests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CriterionField {
    /// `acceptance`: the stage's acceptance criteria.
    #[default]
    Acceptance,
    /// `wiring`: the stage's wiring checks.
    Wiring,
    /// `wiring_tests`: the stage's wiring tests.
    WiringTests,
}

impl CriterionField {
    /// The CLI and serde spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriterionField::Acceptance => "acceptance",
            CriterionField::Wiring => "wiring",
            CriterionField::WiringTests => "wiring-tests",
        }
    }

    /// The label `loom stage complete` prints as `[<label> n]`.
    pub fn gap_label(self) -> &'static str {
        match self {
            CriterionField::Acceptance => "criterion",
            CriterionField::Wiring => "wiring",
            CriterionField::WiringTests => "wiring_tests",
        }
    }

    /// The plan-amendment field that mutates this list.
    pub fn amendment_field(self) -> AmendmentField {
        match self {
            CriterionField::Acceptance => AmendmentField::Acceptance,
            CriterionField::Wiring => AmendmentField::Wiring,
            CriterionField::WiringTests => AmendmentField::WiringTests,
        }
    }
}

/// What a dispute contests (DESIGN D15). `request.md` carries it as a `kind`
/// key beside the kind's own fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DisputeKind {
    /// Entries of `field`'s list, sorted and distinct: `loom stage
    /// dispute-criteria`. One filing names every failing entry so one
    /// adjudication judges them together.
    Criterion {
        criterion_indices: Vec<usize>,
        field: CriterionField,
    },
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
    /// Failing impact-selected tests the stage did not cause:
    /// `loom stage dispute-criteria --field impact-tests`. The filing client
    /// leaves `evidence` empty; the daemon fills it from loom's record.
    ImpactTests {
        test_ids: Vec<String>,
        #[serde(default)]
        evidence: Vec<ImpactSnapshot>,
    },
}

impl DisputeKind {
    /// A criterion dispute over entries `criterion_indices` of `field`'s list.
    pub fn criterion(field: CriterionField, criterion_indices: Vec<usize>) -> Self {
        DisputeKind::Criterion {
            criterion_indices,
            field,
        }
    }

    /// The kind as `request.md` names it.
    pub fn name(&self) -> &'static str {
        match self {
            DisputeKind::Criterion { .. } => "criterion",
            DisputeKind::Findings { .. } => "findings",
            DisputeKind::Contract { .. } => "contract",
            DisputeKind::Integrity { .. } => "integrity",
            DisputeKind::ImpactTests { .. } => "impact-tests",
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

/// The failures `ids` names among the impact-test `failures` loom observed
/// for the stage, under the same rules as [`select_findings`].
pub fn select_impact(failures: &[ImpactFailure], ids: &[String]) -> Result<Vec<ImpactSnapshot>> {
    select(ids, "impact-test failure loom observed", |id| {
        failures
            .iter()
            .find(|failure| failure.id == id)
            .map(|failure| Ok(failure.clone()))
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

/// Most bytes of a criterion's failure output a dispute keeps. The CLI client
/// and the daemon handler both cap at it, so a request carries the same slice
/// whichever side ran the cut.
pub(crate) const FAILURE_OUTPUT_MAX_BYTES: usize = 4096;

/// Most bytes of a dispute's reason the daemon records. The CLI caps a reason
/// at 500 characters; a request from the wire can carry anything.
pub(crate) const DISPUTE_REASON_MAX_BYTES: usize = 4096;

/// `s` reduced to at most `max_bytes`, cut on the last char boundary that fits
/// so the result stays valid UTF-8 (a raw byte slice panics inside a
/// multi-byte character).
pub(crate) fn truncate_to_byte_limit(s: &str, max_bytes: usize) -> String {
    let mut end = max_bytes.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// `s` without its control characters but those in `kept` (`\n` and `\t`
/// for multi-line output): text a stage's code wrote reaches a terminal and
/// an adjudicator's prompt, where an escape sequence could rewrite it.
pub(crate) fn strip_control(s: &str, kept: &[char]) -> String {
    s.chars()
        .filter(|character| !character.is_control() || kept.contains(character))
        .collect()
}

/// The last at most `max_bytes` of `s`, cut on a char boundary: the end of a
/// test run's output is where its verdict and failures are.
pub(crate) fn tail_to_byte_limit(s: &str, max_bytes: usize) -> String {
    s[s.ceil_char_boundary(s.len().saturating_sub(max_bytes))..].to_string()
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
