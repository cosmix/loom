use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

use crate::fs::safe_read::{read_bounded, OverLimit};

use super::comparison_eval;
use super::comparison_schema::{ComparisonArtifact, ComparisonReport, COMPARISON_SCHEMA_VERSION};
use super::UsageArgs;

const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

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
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // The operator names the directory, so it may be reached through a symlink;
    // only the artifact itself is refused as a link.
    let root = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let relative = path.file_name().map(Path::new).unwrap_or(path);
    let bytes = match read_bounded(&root, relative, MAX_INPUT_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            let reason = if error.downcast_ref::<OverLimit>().is_some() {
                "input-too-large"
            } else {
                "input-read-failed"
            };
            return comparison_eval::failure_report(vec![reason]);
        }
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
    fn comparison_refuses_a_symlink_to_a_valid_artifact() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("comparison.json");
        let link = root.path().join("link.json");
        let valid = include_bytes!(
            "../../../tests/fixtures/token_optimization/comparison/valid_improvement.json"
        );
        std::fs::write(&target, valid).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(load_report(&target).verdict, Outcome::SupportedCandidate);
        assert_eq!(load_report(&link).reason_codes, vec!["input-read-failed"]);
    }

    #[test]
    fn comparison_reads_an_artifact_under_a_symlinked_directory() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let linked = root.path().join("linked");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &linked).unwrap();
        let valid = include_bytes!(
            "../../../tests/fixtures/token_optimization/comparison/valid_improvement.json"
        );
        std::fs::write(real.join("comparison.json"), valid).unwrap();

        let report = load_report(&linked.join("comparison.json"));
        assert_eq!(report.verdict, Outcome::SupportedCandidate);
    }

    #[test]
    fn comparison_preserves_the_oversized_input_reason() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("oversized.json");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_INPUT_BYTES as u64 + 1)
            .unwrap();

        assert_eq!(load_report(&path).reason_codes, vec!["input-too-large"]);
    }

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
