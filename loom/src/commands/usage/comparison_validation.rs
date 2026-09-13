use std::collections::{BTreeMap, BTreeSet};

use super::comparison_schema::{
    ComparisonArtifact, ComparisonPair, InterventionDimension, QuotaObservation, RunEvidence,
    MAX_EVIDENCE_ITEMS, MAX_PAIRS,
};
use super::provider_types::{Provider, ProviderTokenVector};

const MAX_IDENTIFIER_BYTES: usize = 160;

pub(super) fn artifact_reasons(artifact: &ComparisonArtifact) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if artifact.pairs.is_empty() {
        reasons.push("no-comparison-pairs");
    }
    if artifact.pairs.len() > MAX_PAIRS {
        reasons.push("pair-limit-exceeded");
    }
    if !valid_id(&artifact.intervention.intervention_id)
        || !valid_id(&artifact.intervention.baseline_revision)
        || !valid_id(&artifact.intervention.candidate_revision)
    {
        reasons.push("invalid-identifier");
    }
    let dimensions = &artifact.intervention.changed_dimensions;
    if dimensions.is_empty()
        || dimensions.len() > 4
        || dimensions.iter().copied().collect::<BTreeSet<_>>().len() != dimensions.len()
        || (artifact.intervention.baseline_revision != artifact.intervention.candidate_revision
            && !dimensions.contains(&InterventionDimension::ImplementationRevision))
    {
        reasons.push("invalid-intervention-manifest");
    }
    reasons
}

pub(super) fn duplicate_run_ids(artifact: &ComparisonArtifact) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for run in artifact
        .pairs
        .iter()
        .flat_map(|pair| [&pair.baseline, &pair.candidate])
    {
        if !seen.insert(run.run_id.clone()) {
            duplicates.insert(run.run_id.clone());
        }
    }
    duplicates
}

pub(super) fn pair_reasons(
    artifact: &ComparisonArtifact,
    pair: &ComparisonPair,
    duplicate_ids: &BTreeSet<String>,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !valid_id(&pair.work_unit_id)
        || !valid_run_ids(&pair.baseline)
        || !valid_run_ids(&pair.candidate)
    {
        reasons.push("invalid-identifier");
    }
    if evidence_limit_exceeded(&pair.baseline) || evidence_limit_exceeded(&pair.candidate) {
        reasons.push("evidence-limit-exceeded");
    }
    if duplicate_ids.contains(&pair.baseline.run_id)
        || duplicate_ids.contains(&pair.candidate.run_id)
    {
        reasons.push("duplicate-run-id");
    }
    validate_revisions(artifact, pair, &mut reasons);
    validate_run(&pair.baseline, &mut reasons);
    validate_run(&pair.candidate, &mut reasons);
    reasons
}

fn valid_run_ids(run: &RunEvidence) -> bool {
    [
        &run.run_id,
        &run.workload_id,
        &run.fixture_id,
        &run.source_input_revision,
        &run.source_input_digest,
        &run.environment_id,
        &run.acceptance_contract_digest,
        &run.implementation_revision,
    ]
    .into_iter()
    .all(|value| valid_id(value))
}

pub(super) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn evidence_limit_exceeded(run: &RunEvidence) -> bool {
    [
        run.assignments.len(),
        run.required_checks.len(),
        run.reviewed_dimensions.len(),
        run.unresolved_findings.len(),
        run.omitted_requirements.len(),
        run.quota_observations.as_ref().map_or(0, Vec::len),
    ]
    .into_iter()
    .any(|count| count > MAX_EVIDENCE_ITEMS)
        || run.retry_count > 10_000
        || run.fix_count > 10_000
}

fn validate_revisions(
    artifact: &ComparisonArtifact,
    pair: &ComparisonPair,
    reasons: &mut Vec<&'static str>,
) {
    if pair.baseline.implementation_revision != artifact.intervention.baseline_revision
        || pair.candidate.implementation_revision != artifact.intervention.candidate_revision
    {
        reasons.push("revision-manifest-mismatch");
    }
}

fn validate_run(run: &RunEvidence, reasons: &mut Vec<&'static str>) {
    if run.assignments.is_empty() {
        reasons.push("missing-provider-assignment");
    }
    if has_duplicate_assignments(run) {
        reasons.push("duplicate-provider-assignment");
    }
    if has_duplicate_evidence(run) {
        reasons.push("duplicate-evidence-identity");
    }
    if !provider_evidence_matches(run) {
        reasons.push("provider-evidence-mismatch");
    }
    if run
        .assignments
        .iter()
        .any(|assignment| !valid_id(&assignment.model) || !valid_id(&assignment.effort))
        || run
            .required_checks
            .iter()
            .any(|check| !valid_id(&check.command_id) || !valid_id(&check.contract_id))
        || run
            .reviewed_dimensions
            .iter()
            .any(|review| !valid_id(&review.dimension))
        || run
            .unresolved_findings
            .iter()
            .any(|finding| !valid_id(&finding.finding_id))
        || run
            .omitted_requirements
            .iter()
            .any(|requirement| !valid_id(requirement))
    {
        reasons.push("invalid-identifier");
    }
    validate_vectors(&run.provider_tokens, reasons);
    if run
        .quota_observations
        .as_deref()
        .is_some_and(|observations| observations.iter().any(invalid_quota))
    {
        reasons.push("invalid-quota-observation");
    }
}

fn has_duplicate_assignments(run: &RunEvidence) -> bool {
    let mut providers = BTreeSet::new();
    run.assignments
        .iter()
        .any(|assignment| !providers.insert(assignment.provider))
}

fn has_duplicate_evidence(run: &RunEvidence) -> bool {
    let mut checks = BTreeSet::new();
    let duplicate_check = run
        .required_checks
        .iter()
        .any(|check| !checks.insert((check.command_id.as_str(), check.contract_id.as_str())));
    let mut reviews = BTreeSet::new();
    let duplicate_review = run
        .reviewed_dimensions
        .iter()
        .any(|review| !reviews.insert(review.dimension.as_str()));
    let mut findings = BTreeSet::new();
    let duplicate_finding = run
        .unresolved_findings
        .iter()
        .any(|finding| !findings.insert(finding.finding_id.as_str()));
    let mut quotas = BTreeSet::new();
    let duplicate_quota = run.quota_observations.as_deref().is_some_and(|items| {
        items
            .iter()
            .any(|item| !quotas.insert((item.provider, item.window.label())))
    });
    duplicate_check || duplicate_review || duplicate_finding || duplicate_quota
}

fn provider_evidence_matches(run: &RunEvidence) -> bool {
    let assigned = run
        .assignments
        .iter()
        .map(|assignment| assignment.provider)
        .collect::<BTreeSet<_>>();
    let token_providers = run.provider_tokens.keys().copied().collect::<BTreeSet<_>>();
    let tokens_match = if run.accounting_complete {
        token_providers == assigned
    } else {
        token_providers.is_subset(&assigned)
    };
    let quota_matches = run.quota_observations.as_deref().is_none_or(|items| {
        items
            .iter()
            .all(|observation| assigned.contains(&observation.provider))
    });
    tokens_match && quota_matches
}

fn validate_vectors(
    vectors: &BTreeMap<Provider, ProviderTokenVector>,
    reasons: &mut Vec<&'static str>,
) {
    if vectors.len() > 2
        || vectors
            .iter()
            .any(|(provider, vector)| invalid_vector(*provider, vector))
    {
        reasons.push("invalid-token-vector");
    }
}

fn invalid_vector(provider: Provider, vector: &ProviderTokenVector) -> bool {
    if vector
        .thinking_output_tokens
        .zip(vector.output_tokens)
        .is_some_and(|(thinking, output)| thinking > output)
    {
        return true;
    }
    match provider {
        Provider::Claude => invalid_claude_vector(vector),
        Provider::Codex => invalid_codex_vector(vector),
    }
}

fn invalid_claude_vector(vector: &ProviderTokenVector) -> bool {
    let fresh_mismatch = vector
        .input_tokens
        .zip(vector.fresh_input_tokens)
        .is_some_and(|(input, fresh)| input != fresh);
    let cache_mismatch = sum3(
        vector.fresh_input_tokens,
        vector.cache_creation_input_tokens,
        vector.cache_read_input_tokens,
    )
    .zip(vector.resident_input_tokens)
    .is_some_and(|(sum, resident)| sum != resident);
    let split_mismatch = sum2(
        vector.cache_write_5m_input_tokens,
        vector.cache_write_1h_input_tokens,
    )
    .zip(vector.cache_creation_input_tokens)
    .is_some_and(|(sum, creation)| sum != creation);
    fresh_mismatch || cache_mismatch || split_mismatch
}

fn invalid_codex_vector(vector: &ProviderTokenVector) -> bool {
    let resident_mismatch = vector
        .input_tokens
        .zip(vector.resident_input_tokens)
        .is_some_and(|(input, resident)| input != resident);
    let input_mismatch = sum2(vector.fresh_input_tokens, vector.cache_read_input_tokens)
        .zip(vector.input_tokens)
        .is_some_and(|(sum, input)| sum != input);
    let total_mismatch = sum2(vector.input_tokens, vector.output_tokens)
        .zip(vector.total_tokens)
        .is_some_and(|(sum, total)| sum != total);
    resident_mismatch || input_mismatch || total_mismatch
}

fn sum2(first: Option<u64>, second: Option<u64>) -> Option<u64> {
    first?.checked_add(second?)
}

fn sum3(first: Option<u64>, second: Option<u64>, third: Option<u64>) -> Option<u64> {
    first?.checked_add(second?)?.checked_add(third?)
}

fn invalid_quota(observation: &QuotaObservation) -> bool {
    !valid_id(&observation.reset_id)
        || observation.observed_start_at >= observation.observed_end_at
        || observation
            .observed_end_at
            .checked_sub(observation.observed_start_at)
            .is_none()
        || observation.resets_at <= observation.observed_start_at
        || !valid_percent(observation.used_percent_start)
        || !valid_percent(observation.used_percent_end)
        || observation.precision_percent.is_some_and(|precision| {
            !precision.is_finite() || precision <= 0.0 || precision > 100.0
        })
}

fn valid_percent(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}
