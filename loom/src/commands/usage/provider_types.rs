use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub(crate) const PROVIDER_LEDGER_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceKind {
    ClaudeTranscript,
    CodexDirect,
    CodexFallback,
    ExecutionReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProvenanceStatus {
    MeasuredCanonical,
    DuplicateExact,
    AmbiguousConflict,
    FallbackCoverage,
    Synthetic,
    ZeroUsage,
    UnknownUsage,
}

impl ProvenanceStatus {
    pub(crate) fn is_measured(self) -> bool {
        self == Self::MeasuredCanonical
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StageAttributionState {
    Known,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Attribution {
    pub(crate) scope: String,
    pub(crate) role: Option<String>,
    pub(crate) stage_id: Option<String>,
    pub(crate) loom_session_id: Option<String>,
    pub(crate) stage_state: StageAttributionState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct ProviderTokenVector {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) fresh_input_tokens: Option<u64>,
    pub(crate) cache_creation_input_tokens: Option<u64>,
    pub(crate) cache_read_input_tokens: Option<u64>,
    pub(crate) cache_write_5m_input_tokens: Option<u64>,
    pub(crate) cache_write_1h_input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) thinking_output_tokens: Option<u64>,
    pub(crate) resident_input_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
}

impl ProviderTokenVector {
    pub(crate) fn is_zero(&self) -> bool {
        [
            self.input_tokens,
            self.cache_creation_input_tokens,
            self.cache_read_input_tokens,
            self.cache_write_5m_input_tokens,
            self.cache_write_1h_input_tokens,
            self.output_tokens,
            self.thinking_output_tokens,
        ]
        .into_iter()
        .flatten()
        .all(|value| value == 0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NormalizedEvent {
    pub(crate) provider: Provider,
    pub(crate) event_timestamp: DateTime<Utc>,
    pub(crate) source_kind: SourceKind,
    pub(crate) request_id: Option<String>,
    pub(crate) source_ordinal: usize,
    pub(crate) model: String,
    pub(crate) project: String,
    pub(crate) tokens: ProviderTokenVector,
    pub(crate) attribution: Attribution,
    pub(crate) provenance: ProvenanceStatus,
    pub(crate) observation_count: usize,
    pub(crate) changed_usage_fields: usize,
    pub(crate) first_observed_in_range: bool,
    pub(crate) true_fresh_start: Option<bool>,
    pub(crate) tool_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub(crate) struct MeasuredTotal {
    pub(crate) value: u64,
    pub(crate) measured_rows: usize,
    pub(crate) unknown_rows: usize,
}

impl MeasuredTotal {
    pub(crate) fn add(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.value = self.value.saturating_add(value);
                self.measured_rows += 1;
            }
            None => self.unknown_rows += 1,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ProviderTotals {
    pub(crate) fresh_input: MeasuredTotal,
    pub(crate) cache_creation: MeasuredTotal,
    pub(crate) cache_read: MeasuredTotal,
    pub(crate) cache_write_5m: MeasuredTotal,
    pub(crate) cache_write_1h: MeasuredTotal,
    pub(crate) output: MeasuredTotal,
    pub(crate) thinking_or_reasoning_output: MeasuredTotal,
    pub(crate) resident_input: MeasuredTotal,
}

impl ProviderTotals {
    pub(crate) fn add(&mut self, tokens: &ProviderTokenVector) {
        self.fresh_input.add(tokens.fresh_input_tokens);
        self.cache_creation.add(tokens.cache_creation_input_tokens);
        self.cache_read.add(tokens.cache_read_input_tokens);
        self.cache_write_5m.add(tokens.cache_write_5m_input_tokens);
        self.cache_write_1h.add(tokens.cache_write_1h_input_tokens);
        self.output.add(tokens.output_tokens);
        self.thinking_or_reasoning_output
            .add(tokens.thinking_output_tokens);
        self.resident_input.add(tokens.resident_input_tokens);
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct CoverageCounts {
    pub(crate) measured_requests: usize,
    pub(crate) measured_responses: usize,
    pub(crate) synthetic_rows: usize,
    pub(crate) zero_rows: usize,
    pub(crate) unknown_usage_rows: usize,
    pub(crate) duplicate_exact_rows: usize,
    pub(crate) ambiguous_rows: usize,
    pub(crate) fallback_coverage_rows: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ProviderDiagnostics {
    pub(crate) files_seen: usize,
    pub(crate) missing_roots: usize,
    pub(crate) unreadable_files: usize,
    pub(crate) malformed_rows: usize,
    pub(crate) missing_timestamp_rows: usize,
    pub(crate) invalid_thinking_rows: usize,
    pub(crate) invalid_token_rows: usize,
    pub(crate) invalid_cache_relations: usize,
    pub(crate) total_token_mismatches: usize,
    pub(crate) streamed_messages: usize,
    pub(crate) stream_first_last_changes: usize,
    pub(crate) stream_changed_fields: usize,
    pub(crate) stream_first_observed: StreamTokenTotals,
    pub(crate) stream_terminal_observed: StreamTokenTotals,
    pub(crate) fallback_duplicate_snapshots: usize,
    pub(crate) mixed_schema_files: usize,
    pub(crate) unknown_fallback_relations: usize,
    pub(crate) receipt_files_seen: usize,
    pub(crate) malformed_receipts: usize,
    pub(crate) unsupported_receipt_versions: usize,
    pub(crate) unattributable_receipts: usize,
}

impl ProviderDiagnostics {
    pub(crate) fn merge(&mut self, other: Self) {
        self.files_seen += other.files_seen;
        self.missing_roots += other.missing_roots;
        self.unreadable_files += other.unreadable_files;
        self.malformed_rows += other.malformed_rows;
        self.missing_timestamp_rows += other.missing_timestamp_rows;
        self.invalid_thinking_rows += other.invalid_thinking_rows;
        self.invalid_token_rows += other.invalid_token_rows;
        self.invalid_cache_relations += other.invalid_cache_relations;
        self.total_token_mismatches += other.total_token_mismatches;
        self.streamed_messages += other.streamed_messages;
        self.stream_first_last_changes += other.stream_first_last_changes;
        self.stream_changed_fields += other.stream_changed_fields;
        self.stream_first_observed
            .merge(other.stream_first_observed);
        self.stream_terminal_observed
            .merge(other.stream_terminal_observed);
        self.fallback_duplicate_snapshots += other.fallback_duplicate_snapshots;
        self.mixed_schema_files += other.mixed_schema_files;
        self.unknown_fallback_relations += other.unknown_fallback_relations;
        self.receipt_files_seen += other.receipt_files_seen;
        self.malformed_receipts += other.malformed_receipts;
        self.unsupported_receipt_versions += other.unsupported_receipt_versions;
        self.unattributable_receipts += other.unattributable_receipts;
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StreamTokenTotals {
    pub(crate) observations: usize,
    pub(crate) input: u64,
    pub(crate) cache_creation: u64,
    pub(crate) cache_read: u64,
    pub(crate) output: u64,
    pub(crate) cache_write_5m: u64,
    pub(crate) cache_write_1h: u64,
}

impl StreamTokenTotals {
    pub(crate) fn add(&mut self, values: [u64; 6]) {
        self.observations += 1;
        self.input = self.input.saturating_add(values[0]);
        self.cache_creation = self.cache_creation.saturating_add(values[1]);
        self.cache_read = self.cache_read.saturating_add(values[2]);
        self.output = self.output.saturating_add(values[3]);
        self.cache_write_5m = self.cache_write_5m.saturating_add(values[4]);
        self.cache_write_1h = self.cache_write_1h.saturating_add(values[5]);
    }

    fn merge(&mut self, other: Self) {
        self.observations += other.observations;
        self.input = self.input.saturating_add(other.input);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.output = self.output.saturating_add(other.output);
        self.cache_write_5m = self.cache_write_5m.saturating_add(other.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.saturating_add(other.cache_write_1h);
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Distribution {
    pub(crate) count: usize,
    pub(crate) min: Option<u64>,
    pub(crate) p50: Option<u64>,
    pub(crate) p95: Option<u64>,
    pub(crate) max: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct FreshStartSummary {
    pub(crate) first_observed_rows: usize,
    pub(crate) true_fresh_starts: usize,
    pub(crate) known_not_fresh_starts: usize,
    pub(crate) unknown: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StageAttributionSummary {
    pub(crate) known: usize,
    pub(crate) unknown: usize,
    pub(crate) not_applicable: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ToolCounts {
    pub(crate) total: usize,
    pub(crate) by_name: BTreeMap<String, usize>,
    pub(crate) unavailable_rows: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct TurnoverRatios {
    pub(crate) cache_read_to_fresh_input: Option<f64>,
    pub(crate) cache_creation_to_fresh_input: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GroupTotals {
    pub(crate) key: String,
    pub(crate) responses: usize,
    pub(crate) totals: ProviderTotals,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ProviderGroupings {
    pub(crate) models: Vec<GroupTotals>,
    pub(crate) projects: Vec<GroupTotals>,
    pub(crate) scopes: Vec<GroupTotals>,
    pub(crate) roles: Vec<GroupTotals>,
    pub(crate) days: Vec<GroupTotals>,
    pub(crate) iso_weeks: Vec<GroupTotals>,
    pub(crate) stages: Vec<GroupTotals>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProviderReport {
    pub(crate) provider: Provider,
    pub(crate) coverage: CoverageCounts,
    pub(crate) totals: ProviderTotals,
    pub(crate) fallback_coverage_totals: ProviderTotals,
    pub(crate) groupings: ProviderGroupings,
    pub(crate) request_length_distribution: Distribution,
    pub(crate) resident_context_distribution: Distribution,
    pub(crate) fresh_starts: FreshStartSummary,
    pub(crate) tools: ToolCounts,
    pub(crate) stage_attribution: StageAttributionSummary,
    pub(crate) turnover: TurnoverRatios,
    pub(crate) diagnostics: ProviderDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProviderLedger {
    pub(crate) schema_version: u16,
    pub(crate) range: TimeRangeView,
    pub(crate) providers: Vec<ProviderReport>,
    pub(crate) rows: Vec<NormalizedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quota_history: Option<super::quota_history::QuotaHistorySection>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct TimeRangeView {
    pub(crate) since: DateTime<Utc>,
    pub(crate) until: Option<DateTime<Utc>>,
}
