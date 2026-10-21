//! `loom usage --run-evidence`: writes one run's `RunEvidence`, the input
//! `--compare` pairs. The declaration file supplies the identities the tooling
//! cannot measure; token vectors, retries, fixes, the critical path and open
//! review findings are measured from the project's ledger and stage records.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use super::comparison_schema::{
    AccountingProvenance, CheckEvidence, EvidenceVerdict, ProviderAssignment, QuotaObservation,
    ReviewEvidence, RunEvidence,
};
use super::completeness::CompletenessState;
use super::provider_types::{NormalizedEvent, Provider, ProviderLedger, ProviderTokenVector};
use super::run_evidence_stages::stage_evidence;
use super::time_range::TimeRange;
use super::{normalize_usage, usage_work_dir, ProviderSelection, UsageArgs};

const MAX_DECLARATION_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunDeclaration {
    run_id: String,
    workload_id: String,
    fixture_id: String,
    source_input_revision: String,
    source_input_digest: String,
    environment_id: String,
    acceptance_contract_digest: String,
    implementation_revision: String,
    assignments: Vec<ProviderAssignment>,
    accepted: Option<bool>,
    required_checks: Vec<CheckEvidence>,
    reviewed_dimensions: Vec<ReviewEvidence>,
    omitted_requirements: Vec<String>,
    semantic_completion: EvidenceVerdict,
    quota_observations: Option<Vec<QuotaObservation>>,
}

pub(super) fn produce(args: &UsageArgs, declaration: &Path) -> Result<()> {
    let declaration = read_declaration(declaration)?;
    let range = run_window(args)?;
    let work_dir = usage_work_dir(args.project.as_deref(), false)
        .ok_or_else(|| anyhow!("the project has no .loom/work"))?;
    let normalized = normalize_usage(args, ProviderSelection::All, range, Some(&work_dir))?;
    let ledger = &normalized.ledger;
    let assigned: BTreeSet<Provider> = declaration
        .assignments
        .iter()
        .map(|assignment| assignment.provider)
        .collect();
    refuse_undeclared(ledger, &assigned)?;
    let provider_tokens = provider_tokens(ledger, &assigned);
    let provenance = accounting_provenance(ledger, &assigned, &provider_tokens);
    let stages = stage_evidence(&work_dir, &range)?;
    let evidence = RunEvidence {
        run_id: declaration.run_id,
        workload_id: declaration.workload_id,
        fixture_id: declaration.fixture_id,
        source_input_revision: declaration.source_input_revision,
        source_input_digest: declaration.source_input_digest,
        environment_id: declaration.environment_id,
        acceptance_contract_digest: declaration.acceptance_contract_digest,
        implementation_revision: declaration.implementation_revision,
        assignments: declaration.assignments,
        accepted: declaration.accepted,
        required_checks: declaration.required_checks,
        reviewed_dimensions: declaration.reviewed_dimensions,
        unresolved_findings: stages.unresolved_findings,
        omitted_requirements: declaration.omitted_requirements,
        retry_count: stages.retry_count,
        fix_count: stages.fix_count,
        semantic_completion: declaration.semantic_completion,
        critical_path_ms: stages.critical_path_ms,
        // No producer of notification latency exists; `--compare` reports it unmeasured.
        notification_latency_ms: None,
        accounting_complete: provenance == AccountingProvenance::Measured,
        accounting_provenance: provenance,
        provider_tokens,
        quota_observations: declaration.quota_observations,
    };
    print_evidence(&evidence)
}

fn read_declaration(path: &Path) -> Result<RunDeclaration> {
    let file =
        File::open(path).with_context(|| format!("reading the declaration {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_DECLARATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading the declaration {}", path.display()))?;
    if bytes.len() as u64 > MAX_DECLARATION_BYTES {
        bail!(
            "the declaration {} exceeds {MAX_DECLARATION_BYTES} bytes",
            path.display()
        );
    }
    // The serde message can quote a value, so only the position is reported.
    serde_json::from_slice(&bytes).map_err(|error| {
        anyhow!(
            "the declaration {} is invalid at line {} column {}",
            path.display(),
            error.line(),
            error.column()
        )
    })
}

/// A relative window cannot be reproduced, so both bounds must be RFC3339.
fn run_window(args: &UsageArgs) -> Result<TimeRange> {
    let since = rfc3339_flag("--since", args.since.as_deref())?;
    let until = rfc3339_flag("--until", args.until.as_deref())?;
    TimeRange::parse(since, Some(until), chrono::Utc::now())
}

fn rfc3339_flag<'a>(flag: &str, value: Option<&'a str>) -> Result<&'a str> {
    let value = value.ok_or_else(|| anyhow!("{flag} is required with --run-evidence"))?;
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| anyhow!("{flag} must be an RFC3339 instant with --run-evidence"))?;
    Ok(value)
}

/// A provider the declaration does not assign must contribute no row: its
/// usage would otherwise vanish from an artifact that reads as complete.
fn refuse_undeclared(ledger: &ProviderLedger, assigned: &BTreeSet<Provider>) -> Result<()> {
    let mut rows: BTreeMap<Provider, usize> = BTreeMap::new();
    for row in ledger
        .rows
        .iter()
        .filter(|row| !assigned.contains(&row.provider))
    {
        *rows.entry(row.provider).or_default() += 1;
    }
    if rows.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|(provider, count)| {
            format!(
                "undeclared provider {}: {count} in-scope usage rows; assign it in the declaration",
                provider.name()
            )
        })
        .collect();
    bail!("{}", lines.join("\n"))
}

fn provider_tokens(
    ledger: &ProviderLedger,
    assigned: &BTreeSet<Provider>,
) -> BTreeMap<Provider, ProviderTokenVector> {
    assigned
        .iter()
        .filter_map(|provider| {
            let rows: Vec<&NormalizedEvent> = ledger
                .rows
                .iter()
                .filter(|row| row.provider == *provider && row.provenance.is_measured())
                .collect();
            (!rows.is_empty()).then(|| (*provider, sum_vectors(&rows)))
        })
        .collect()
}

/// A field is the sum only when every row carries it: unknown is not zero.
fn sum_vectors(rows: &[&NormalizedEvent]) -> ProviderTokenVector {
    let sum = |get: fn(&ProviderTokenVector) -> Option<u64>| {
        rows.iter().try_fold(0u64, |total, row| {
            get(&row.tokens).map(|value| total.saturating_add(value))
        })
    };
    ProviderTokenVector {
        input_tokens: sum(|t| t.input_tokens),
        fresh_input_tokens: sum(|t| t.fresh_input_tokens),
        cache_creation_input_tokens: sum(|t| t.cache_creation_input_tokens),
        cache_read_input_tokens: sum(|t| t.cache_read_input_tokens),
        cache_write_5m_input_tokens: sum(|t| t.cache_write_5m_input_tokens),
        cache_write_1h_input_tokens: sum(|t| t.cache_write_1h_input_tokens),
        output_tokens: sum(|t| t.output_tokens),
        thinking_output_tokens: sum(|t| t.thinking_output_tokens),
        resident_input_tokens: sum(|t| t.resident_input_tokens),
        total_tokens: sum(|t| t.total_tokens),
    }
}

fn accounting_provenance(
    ledger: &ProviderLedger,
    assigned: &BTreeSet<Provider>,
    tokens: &BTreeMap<Provider, ProviderTokenVector>,
) -> AccountingProvenance {
    if assigned
        .iter()
        .any(|provider| !tokens.contains_key(provider))
    {
        return AccountingProvenance::Unknown;
    }
    let partial = ledger.providers.iter().any(|report| {
        assigned.contains(&report.provider)
            && report.completeness.state == CompletenessState::Partial
    });
    if partial {
        AccountingProvenance::Partial
    } else {
        AccountingProvenance::Measured
    }
}

fn print_evidence(evidence: &RunEvidence) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, evidence).context("writing the run evidence")?;
    writeln!(stdout).context("writing the run evidence")
}

#[cfg(test)]
#[path = "run_evidence_tests.rs"]
mod tests;
