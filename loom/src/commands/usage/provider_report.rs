use std::collections::BTreeMap;

use chrono::Datelike;

use super::provider_types::{
    CoverageCounts, Distribution, FreshStartSummary, GroupTotals, NormalizedEvent,
    ProvenanceStatus, Provider, ProviderDiagnostics, ProviderGroupings, ProviderReport,
    ProviderTotals, SourceKind, StageAttributionState, StageAttributionSummary, ToolCounts,
    TurnoverRatios,
};

#[derive(Default)]
struct GroupMaps {
    models: BTreeMap<String, GroupTotals>,
    projects: BTreeMap<String, GroupTotals>,
    scopes: BTreeMap<String, GroupTotals>,
    roles: BTreeMap<String, GroupTotals>,
    days: BTreeMap<String, GroupTotals>,
    iso_weeks: BTreeMap<String, GroupTotals>,
    stages: BTreeMap<String, GroupTotals>,
}

pub(crate) fn build(
    provider: Provider,
    rows: &[NormalizedEvent],
    diagnostics: ProviderDiagnostics,
) -> ProviderReport {
    let provider_rows: Vec<_> = rows.iter().filter(|row| row.provider == provider).collect();
    let coverage = coverage(&provider_rows);
    let measured: Vec<_> = provider_rows
        .iter()
        .copied()
        .filter(|row| row.provenance.is_measured())
        .collect();
    let measured_totals = totals(&measured);
    let fallback = provider_rows
        .iter()
        .copied()
        .filter(|row| row.provenance == ProvenanceStatus::FallbackCoverage)
        .collect::<Vec<_>>();
    ProviderReport {
        provider,
        coverage,
        fallback_coverage_totals: totals(&fallback),
        groupings: groupings(&measured),
        request_length_distribution: distribution(
            measured
                .iter()
                .filter_map(|row| row.tokens.fresh_input_tokens)
                .collect(),
        ),
        resident_context_distribution: distribution(
            measured
                .iter()
                .filter_map(|row| row.tokens.resident_input_tokens)
                .collect(),
        ),
        fresh_starts: fresh_starts(&measured),
        tools: tool_counts(&measured),
        stage_attribution: stage_attribution(&measured),
        turnover: turnover(&measured_totals),
        totals: measured_totals,
        diagnostics,
    }
}

fn coverage(rows: &[&NormalizedEvent]) -> CoverageCounts {
    let mut counts = CoverageCounts::default();
    for row in rows {
        match row.provenance {
            ProvenanceStatus::MeasuredCanonical => {
                counts.measured_requests += 1;
                counts.measured_responses += 1;
            }
            ProvenanceStatus::Synthetic => counts.synthetic_rows += 1,
            ProvenanceStatus::ZeroUsage => counts.zero_rows += 1,
            ProvenanceStatus::UnknownUsage => counts.unknown_usage_rows += 1,
            ProvenanceStatus::DuplicateExact => counts.duplicate_exact_rows += 1,
            ProvenanceStatus::AmbiguousConflict => counts.ambiguous_rows += 1,
            ProvenanceStatus::FallbackCoverage => counts.fallback_coverage_rows += 1,
        }
    }
    counts
}

fn totals(rows: &[&NormalizedEvent]) -> ProviderTotals {
    rows.iter()
        .fold(ProviderTotals::default(), |mut total, row| {
            total.add(&row.tokens);
            total
        })
}

fn groupings(rows: &[&NormalizedEvent]) -> ProviderGroupings {
    let mut maps = GroupMaps::default();
    for row in rows {
        add_group(&mut maps.models, &row.model, row);
        add_group(&mut maps.projects, &row.project, row);
        add_group(&mut maps.scopes, &row.attribution.scope, row);
        add_group(
            &mut maps.roles,
            row.attribution.role.as_deref().unwrap_or("unknown"),
            row,
        );
        add_group(
            &mut maps.days,
            &row.event_timestamp.date_naive().to_string(),
            row,
        );
        let week = row.event_timestamp.iso_week();
        add_group(
            &mut maps.iso_weeks,
            &format!("{}-W{:02}", week.year(), week.week()),
            row,
        );
        add_group(
            &mut maps.stages,
            row.attribution.stage_id.as_deref().unwrap_or("unknown"),
            row,
        );
    }
    ProviderGroupings {
        models: maps.models.into_values().collect(),
        projects: maps.projects.into_values().collect(),
        scopes: maps.scopes.into_values().collect(),
        roles: maps.roles.into_values().collect(),
        days: maps.days.into_values().collect(),
        iso_weeks: maps.iso_weeks.into_values().collect(),
        stages: maps.stages.into_values().collect(),
    }
}

fn add_group(map: &mut BTreeMap<String, GroupTotals>, key: &str, row: &NormalizedEvent) {
    let group = map.entry(key.to_owned()).or_insert_with(|| GroupTotals {
        key: key.to_owned(),
        responses: 0,
        totals: ProviderTotals::default(),
    });
    group.responses += 1;
    group.totals.add(&row.tokens);
}

fn distribution(mut values: Vec<u64>) -> Distribution {
    if values.is_empty() {
        return Distribution::default();
    }
    values.sort_unstable();
    let last = values.len() - 1;
    Distribution {
        count: values.len(),
        min: values.first().copied(),
        p50: values.get(last / 2).copied(),
        p95: values.get(last.saturating_mul(95) / 100).copied(),
        max: values.last().copied(),
    }
}

fn fresh_starts(rows: &[&NormalizedEvent]) -> FreshStartSummary {
    let mut summary = FreshStartSummary::default();
    for row in rows.iter().filter(|row| row.first_observed_in_range) {
        summary.first_observed_rows += 1;
        match row.true_fresh_start {
            Some(true) => summary.true_fresh_starts += 1,
            Some(false) => summary.known_not_fresh_starts += 1,
            None => summary.unknown += 1,
        }
    }
    summary
}

fn tool_counts(rows: &[&NormalizedEvent]) -> ToolCounts {
    let mut tools = ToolCounts::default();
    for row in rows {
        if matches!(
            row.source_kind,
            SourceKind::CodexDirect | SourceKind::CodexFallback | SourceKind::ExecutionReceipt
        ) {
            tools.unavailable_rows += 1;
            continue;
        }
        tools.total += row.tool_names.len();
        for name in &row.tool_names {
            *tools.by_name.entry(name.clone()).or_default() += 1;
        }
    }
    tools
}

fn stage_attribution(rows: &[&NormalizedEvent]) -> StageAttributionSummary {
    let mut summary = StageAttributionSummary::default();
    for row in rows {
        match row.attribution.stage_state {
            StageAttributionState::Known => summary.known += 1,
            StageAttributionState::Unknown => summary.unknown += 1,
            StageAttributionState::NotApplicable => summary.not_applicable += 1,
        }
    }
    summary
}

fn turnover(totals: &ProviderTotals) -> TurnoverRatios {
    let denominator = totals.fresh_input.value;
    TurnoverRatios {
        cache_read_to_fresh_input: ratio(
            totals.cache_read.value,
            totals.cache_read.measured_rows,
            denominator,
        ),
        cache_creation_to_fresh_input: ratio(
            totals.cache_creation.value,
            totals.cache_creation.measured_rows,
            denominator,
        ),
    }
}

fn ratio(numerator: u64, measured_rows: usize, denominator: u64) -> Option<f64> {
    if measured_rows > 0 && denominator > 0 {
        Some(numerator as f64 / denominator as f64)
    } else {
        None
    }
}

pub(crate) fn render(ledger: &super::provider_types::ProviderLedger) {
    use super::sections::fmt::{format_u64, heading, row};

    heading("Provider ledger (measured tokens)");
    for report in &ledger.providers {
        let provider = report.provider.name();
        row(
            &format!("{provider} responses"),
            report.coverage.measured_responses,
        );
        row(
            &format!("{provider} fresh input"),
            format_u64(report.totals.fresh_input.value),
        );
        row(
            &format!("{provider} cache reads"),
            format_u64(report.totals.cache_read.value),
        );
        row(
            &format!("{provider} output"),
            format_u64(report.totals.output.value),
        );
    }
    if let Some(history) = &ledger.quota_history {
        for provider in &history.providers {
            row(
                &format!("{} quota history", provider.provider.name()),
                quota_history_summary(provider),
            );
        }
    }
}

fn quota_history_summary(history: &super::quota_history::ProviderQuotaHistory) -> String {
    format!(
        "{} ({} points)",
        history.source.name(),
        history.points.len()
    )
}

#[cfg(test)]
#[path = "provider_report_tests.rs"]
mod tests;
