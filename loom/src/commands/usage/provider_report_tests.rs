use chrono::{TimeZone, Utc};

use super::*;
use crate::commands::usage::provider_types::{
    Attribution, ProviderTokenVector, StageAttributionState,
};

fn event(provenance: ProvenanceStatus, fresh: Option<u64>) -> NormalizedEvent {
    NormalizedEvent {
        provider: Provider::Claude,
        event_timestamp: Utc
            .with_ymd_and_hms(2026, 9, 12, 20, 0, 0)
            .single()
            .expect("valid fixture timestamp"),
        source_kind: SourceKind::ClaudeTranscript,
        request_id: Some("id".to_owned()),
        source_ordinal: 1,
        model: "claude".to_owned(),
        project: "project".to_owned(),
        tokens: ProviderTokenVector {
            input_tokens: fresh,
            fresh_input_tokens: fresh,
            cache_creation_input_tokens: Some(5),
            cache_read_input_tokens: Some(10),
            cache_write_5m_input_tokens: Some(5),
            cache_write_1h_input_tokens: Some(0),
            output_tokens: Some(2),
            thinking_output_tokens: None,
            resident_input_tokens: fresh.map(|value| value + 15),
            total_tokens: None,
        },
        attribution: Attribution {
            scope: "subagent".to_owned(),
            role: Some("review".to_owned()),
            stage_id: Some("stage-a".to_owned()),
            loom_session_id: Some("loom-1".to_owned()),
            stage_state: StageAttributionState::Known,
        },
        provenance,
        observation_count: 1,
        changed_usage_fields: 0,
        first_observed_in_range: true,
        true_fresh_start: Some(true),
        tool_names: vec!["Read".to_owned()],
        codex_thread_id: None,
        codex_thread_conflict: false,
        forward_candidate: false,
        forward_receipt: None,
    }
}

#[test]
fn report_totals_groupings_and_ratios_use_only_measured_rows() {
    let rows = vec![
        event(ProvenanceStatus::MeasuredCanonical, Some(20)),
        event(ProvenanceStatus::AmbiguousConflict, Some(999)),
        event(ProvenanceStatus::ZeroUsage, Some(0)),
    ];

    let report = build(Provider::Claude, &rows, ProviderDiagnostics::default());

    assert_eq!(report.coverage.measured_responses, 1);
    assert_eq!(report.coverage.ambiguous_rows, 1);
    assert_eq!(report.coverage.zero_rows, 1);
    assert_eq!(report.totals.fresh_input.value, 20);
    assert_eq!(report.groupings.models[0].responses, 1);
    assert_eq!(report.fresh_starts.true_fresh_starts, 1);
    assert_eq!(report.tools.by_name["Read"], 1);
    assert_eq!(report.stage_attribution.known, 1);
    assert_eq!(report.turnover.cache_read_to_fresh_input, Some(0.5));
}

#[test]
fn absent_optional_field_is_counted_unknown() {
    let row = event(ProvenanceStatus::MeasuredCanonical, Some(20));

    let report = build(Provider::Claude, &[row], ProviderDiagnostics::default());

    assert_eq!(report.totals.thinking_or_reasoning_output.value, 0);
    assert_eq!(report.totals.thinking_or_reasoning_output.unknown_rows, 1);
}

#[test]
fn quota_history_text_reports_state_and_point_count() {
    let history = super::super::quota_history::ProviderQuotaHistory::unavailable(Provider::Claude);

    assert_eq!(quota_history_summary(&history), "unavailable (0 points)");
}
