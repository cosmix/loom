use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::provider_types::{Provider, ProviderTokenVector};
use crate::quota::{HistoryContinuity, WindowKind};

pub(super) const COMPARISON_SCHEMA_VERSION: u16 = 1;
pub(super) const MAX_PAIRS: usize = 1_024;
pub(super) const MAX_EVIDENCE_ITEMS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComparisonArtifact {
    pub(super) schema_version: u16,
    pub(super) intervention: Intervention,
    pub(super) pairs: Vec<ComparisonPair>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intervention {
    pub(super) intervention_id: String,
    pub(super) baseline_revision: String,
    pub(super) candidate_revision: String,
    pub(super) changed_dimensions: Vec<InterventionDimension>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum InterventionDimension {
    ImplementationRevision,
    ProviderAssignment,
    ModelAssignment,
    EffortAssignment,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComparisonPair {
    pub(super) work_unit_id: String,
    pub(super) baseline: RunEvidence,
    pub(super) candidate: RunEvidence,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunEvidence {
    pub(super) run_id: String,
    pub(super) workload_id: String,
    pub(super) fixture_id: String,
    pub(super) source_input_revision: String,
    pub(super) source_input_digest: String,
    pub(super) environment_id: String,
    pub(super) acceptance_contract_digest: String,
    pub(super) implementation_revision: String,
    pub(super) assignments: Vec<ProviderAssignment>,
    pub(super) accepted: Option<bool>,
    pub(super) required_checks: Vec<CheckEvidence>,
    pub(super) reviewed_dimensions: Vec<ReviewEvidence>,
    pub(super) unresolved_findings: Vec<FindingEvidence>,
    pub(super) omitted_requirements: Vec<String>,
    pub(super) retry_count: u32,
    pub(super) fix_count: u32,
    pub(super) semantic_completion: EvidenceVerdict,
    pub(super) critical_path_ms: Option<u64>,
    pub(super) notification_latency_ms: Option<u64>,
    pub(super) accounting_complete: bool,
    pub(super) accounting_provenance: AccountingProvenance,
    pub(super) provider_tokens: BTreeMap<Provider, ProviderTokenVector>,
    pub(super) quota_observations: Option<Vec<QuotaObservation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProviderAssignment {
    pub(super) provider: Provider,
    pub(super) model: String,
    pub(super) effort: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckEvidence {
    pub(super) command_id: String,
    pub(super) contract_id: String,
    pub(super) verdict: EvidenceVerdict,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewEvidence {
    pub(super) dimension: String,
    pub(super) independent: bool,
    pub(super) verdict: EvidenceVerdict,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FindingEvidence {
    pub(super) finding_id: String,
    pub(super) severity: FindingSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum FindingSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum EvidenceVerdict {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum AccountingProvenance {
    Measured,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QuotaObservation {
    pub(super) provider: Provider,
    pub(super) window: WindowKind,
    pub(super) reset_id: String,
    pub(super) resets_at: i64,
    pub(super) observed_start_at: i64,
    pub(super) observed_end_at: i64,
    pub(super) used_percent_start: f64,
    pub(super) used_percent_end: f64,
    pub(super) precision_percent: Option<f64>,
    pub(super) unrelated_concurrent_use: Option<bool>,
    pub(super) continuity: HistoryContinuity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Outcome {
    Rejected,
    Inconclusive,
    SupportedCandidate,
}

impl Outcome {
    pub(super) fn worse(self, other: Self) -> Self {
        match (self, other) {
            (Self::Rejected, _) | (_, Self::Rejected) => Self::Rejected,
            (Self::Inconclusive, _) | (_, Self::Inconclusive) => Self::Inconclusive,
            _ => Self::SupportedCandidate,
        }
    }

    pub(super) fn exit_code(self) -> i32 {
        match self {
            Self::SupportedCandidate => 0,
            Self::Rejected => 1,
            Self::Inconclusive => 2,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Rejected => "rejected",
            Self::Inconclusive => "inconclusive",
            Self::SupportedCandidate => "supported-candidate",
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct PairResult {
    pub(super) work_unit_id: String,
    pub(super) verdict: Outcome,
    pub(super) token_proxy_verdict: Outcome,
    pub(super) subscription_verdict: Outcome,
    pub(super) reason_codes: Vec<&'static str>,
    pub(super) baseline_provider_tokens: BTreeMap<Provider, ProviderTokenVector>,
    pub(super) candidate_provider_tokens: BTreeMap<Provider, ProviderTokenVector>,
    pub(super) quota_comparisons: Vec<QuotaComparisonEvidence>,
}

#[derive(Debug, Serialize)]
pub(super) struct QuotaComparisonEvidence {
    pub(super) provider: Provider,
    pub(super) window: WindowKind,
    pub(super) baseline_consumption_percent: Option<f64>,
    pub(super) candidate_consumption_percent: Option<f64>,
    pub(super) combined_uncertainty_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(super) struct ComparisonReport {
    pub(super) schema_version: u16,
    pub(super) verdict: Outcome,
    pub(super) token_proxy_verdict: Outcome,
    pub(super) subscription_verdict: Outcome,
    pub(super) reason_codes: Vec<&'static str>,
    pub(super) pairs: Vec<PairResult>,
    pub(super) claim_scope: &'static str,
}
