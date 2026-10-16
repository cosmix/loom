//! Pure assertion semantics shared by all command verification surfaces.

use super::result::CriterionResult;

#[derive(Default)]
pub(crate) struct AssertionSpec<'a> {
    pub expected_exit: i32,
    pub stdout_contains: &'a [String],
    pub stdout_not_contains: &'a [String],
    pub stderr_contains: &'a [String],
    pub stderr_empty: bool,
}

pub(crate) struct Evaluation {
    pub passed: bool,
    pub output_complete: bool,
    pub exit_code_matched: bool,
    pub stdout_contains_matched: Vec<bool>,
    pub stdout_not_contains_matched: Vec<bool>,
    pub stderr_empty_matched: Option<bool>,
    pub failures: Vec<String>,
}

pub(crate) fn evaluate(spec: &AssertionSpec<'_>, result: &CriterionResult) -> Evaluation {
    let (output_complete, exit_code_matched, mut failures) = execution_failures(spec, result);
    let stdout_contains_matched = patterns(
        &result.stdout,
        spec.stdout_contains,
        true,
        "stdout",
        &mut failures,
    );
    let stdout_not_contains_matched = patterns(
        &result.stdout,
        spec.stdout_not_contains,
        false,
        "stdout",
        &mut failures,
    );
    patterns(
        &result.stderr,
        spec.stderr_contains,
        true,
        "stderr",
        &mut failures,
    );
    let stderr_empty_matched = spec.stderr_empty.then_some(result.stderr.is_empty());
    if stderr_empty_matched == Some(false) {
        failures.push("stderr was not empty".into());
    }
    Evaluation {
        passed: failures.is_empty(),
        output_complete,
        exit_code_matched,
        stdout_contains_matched,
        stdout_not_contains_matched,
        stderr_empty_matched,
        failures,
    }
}

fn execution_failures(
    spec: &AssertionSpec<'_>,
    result: &CriterionResult,
) -> (bool, bool, Vec<String>) {
    let mut failures = Vec::new();
    if result.timed_out {
        failures.push("command timed out".into());
    }
    let output_complete = result.output_complete;
    if !output_complete {
        failures.push("output capture incomplete; assertions cannot be certified".into());
    }
    let exit_code_matched = result.exit_code == Some(spec.expected_exit);
    if !exit_code_matched {
        failures.push(format!(
            "expected exit code {}, got {:?}",
            spec.expected_exit, result.exit_code
        ));
    }
    (output_complete, exit_code_matched, failures)
}

fn patterns(
    output: &str,
    patterns: &[String],
    required: bool,
    stream: &str,
    failures: &mut Vec<String>,
) -> Vec<bool> {
    patterns
        .iter()
        .map(|pattern| {
            let matched = output.contains(pattern) == required;
            if !matched {
                let reason = if required {
                    "missing expected pattern"
                } else {
                    "contains forbidden pattern"
                };
                failures.push(format!("{stream} {reason} '{pattern}'"));
            }
            matched
        })
        .collect()
}
