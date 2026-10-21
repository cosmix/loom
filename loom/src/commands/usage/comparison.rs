use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::Result;

use super::comparison_eval;
use super::comparison_schema::{ComparisonArtifact, ComparisonReport, COMPARISON_SCHEMA_VERSION};
use super::UsageArgs;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn compare(path: &Path, args: &UsageArgs) -> Result<i32> {
    let report = if args.has_explicit_selection() {
        comparison_eval::failure_report(vec!["comparison-selection-conflict"])
    } else {
        load_report(path)
    };
    let exit_code = report.verdict.exit_code();
    if args.json {
        render_json(&report)?;
    } else {
        render_human(&report)?;
    }
    Ok(exit_code)
}

fn load_report(path: &Path) -> ComparisonReport {
    let bytes = match read_bounded(path) {
        Ok(bytes) => bytes,
        Err(reason) => return comparison_eval::failure_report(vec![reason]),
    };
    let artifact: ComparisonArtifact = match serde_json::from_slice(&bytes) {
        Ok(artifact) => artifact,
        Err(_) => return comparison_eval::failure_report(vec!["malformed-input"]),
    };
    if artifact.schema_version != COMPARISON_SCHEMA_VERSION {
        return comparison_eval::failure_report(vec!["unsupported-schema-version"]);
    }
    comparison_eval::evaluate(artifact)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, &'static str> {
    let file = File::open(path).map_err(|_| "input-read-failed")?;
    let metadata = file.metadata().map_err(|_| "input-read-failed")?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err("input-too-large");
    }
    let capacity = usize::try_from(metadata.len().min(MAX_INPUT_BYTES)).unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input-read-failed")?;
    let too_large = match u64::try_from(bytes.len()) {
        Ok(length) => length > MAX_INPUT_BYTES,
        Err(_) => true,
    };
    if too_large {
        Err("input-too-large")
    } else {
        Ok(bytes)
    }
}

fn render_json(report: &ComparisonReport) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, report)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn render_human(report: &ComparisonReport) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    write_human(&mut output, report)?;
    output.flush()?;
    Ok(())
}

fn write_human(output: &mut dyn Write, report: &ComparisonReport) -> Result<()> {
    writeln!(
        output,
        "comparison: {}; token proxy: {}; subscription: {}",
        report.verdict.name(),
        report.token_proxy_verdict.name(),
        report.subscription_verdict.name()
    )?;
    for pair in &report.pairs {
        writeln!(
            output,
            "{}: {}{}",
            pair.work_unit_id,
            pair.verdict.name(),
            human_reasons(&pair.reason_codes)
        )?;
    }
    // Report-level codes (net total, latency tolerance, ...) that no pair line
    // already printed.
    let unshown: Vec<&str> = report
        .reason_codes
        .iter()
        .copied()
        .filter(|code| {
            !report
                .pairs
                .iter()
                .any(|pair| pair.reason_codes.contains(code))
        })
        .collect();
    if !unshown.is_empty() {
        writeln!(output, "reasons: {}", unshown.join(", "))?;
    }
    Ok(())
}

fn human_reasons(reasons: &[&str]) -> String {
    if reasons.is_empty() {
        String::new()
    } else {
        format!(" ({})", reasons.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::usage::comparison_schema::{Outcome, PairResult};

    #[test]
    fn report_level_reason_prints_beside_pair_lines() {
        let mut report =
            comparison_eval::failure_report(vec!["pair-shared", "net-total-regression"]);
        report.verdict = Outcome::Rejected;
        report.pairs.push(PairResult {
            work_unit_id: "unit-a".to_string(),
            verdict: Outcome::SupportedCandidate,
            token_proxy_verdict: Outcome::SupportedCandidate,
            subscription_verdict: Outcome::SupportedCandidate,
            reason_codes: vec!["pair-shared"],
            baseline_provider_tokens: Default::default(),
            candidate_provider_tokens: Default::default(),
            quota_comparisons: Vec::new(),
        });
        let mut out = Vec::new();
        write_human(&mut out, &report).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("unit-a: supported-candidate (pair-shared)"),
            "{text}"
        );
        assert!(text.contains("reasons: net-total-regression"), "{text}");
        assert!(!text.contains("reasons: pair-shared"), "{text}");
    }
}
