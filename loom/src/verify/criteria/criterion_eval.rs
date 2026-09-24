//! Full evaluation of simple and extended acceptance assertions.

use std::path::Path;
use std::time::Duration;

use super::cache_contract::{AssertionVerdict, CriterionContract};
use super::result::CriterionResult;
use super::zero_tests::{tests_executed, zero_test_failure};
use crate::models::stage::AcceptanceCriterion;

/// A criterion's command as the plan wrote it, which failures name, and as the
/// zero-test guard recognises it: variables expanded, setup left out, run in
/// `cwd`.
pub(super) struct CriterionCommand<'a> {
    pub(super) written: &'a str,
    pub(super) expanded: &'a str,
    pub(super) cwd: &'a Path,
}

/// The verdict on one run and the failures it reports. Under the zero-test
/// guard the verdict also counts the tests the run executed.
pub(super) fn evaluate(
    criterion: &AcceptanceCriterion,
    contract: &CriterionContract,
    result: &CriterionResult,
    command: &CriterionCommand<'_>,
) -> (AssertionVerdict, Vec<String>) {
    let executed = (contract.guards_zero_tests() && !result.timed_out)
        .then(|| tests_executed(command.expanded, command.cwd, result))
        .flatten();
    let verdict = contract.verdict_with_tests(result, executed);
    let failures = check_criterion(criterion, contract, result, command, &verdict);
    (verdict, failures)
}

fn check_criterion(
    criterion: &AcceptanceCriterion,
    contract: &CriterionContract,
    result: &CriterionResult,
    command: &CriterionCommand<'_>,
    verdict: &AssertionVerdict,
) -> Vec<String> {
    let mut failures = match criterion {
        AcceptanceCriterion::Simple(_) => {
            check_simple(result, command.written, contract.timeout(), verdict)
        }
        AcceptanceCriterion::Extended(_) => {
            check_extended_criterion(contract, result, command.written, verdict)
        }
    };
    if contract.rejects_zero_tests(verdict.tests_executed) {
        let failure = zero_test_failure(command.expanded, command.cwd, result);
        failures.extend(failure.map(|reason| format!("Command '{}': {reason}", command.written)));
    }
    failures
}

fn check_simple(
    result: &CriterionResult,
    command: &str,
    timeout: Duration,
    verdict: &AssertionVerdict,
) -> Vec<String> {
    if verdict.timed_out {
        return vec![format!(
            "Command '{command}' timed out after {}s",
            timeout.as_secs()
        )];
    }
    if verdict.exit_code_matched {
        Vec::new()
    } else {
        vec![format!(
            "Command '{command}' failed with exit code {:?}",
            result.exit_code
        )]
    }
}

fn check_extended_criterion(
    contract: &CriterionContract,
    result: &CriterionResult,
    command: &str,
    verdict: &AssertionVerdict,
) -> Vec<String> {
    if verdict.timed_out {
        return vec![format!(
            "Command '{command}' timed out after {}s",
            contract.timeout().as_secs()
        )];
    }
    let mut failures = Vec::new();
    check_exit(contract, result, command, verdict, &mut failures);
    check_stdout(contract, command, verdict, &mut failures);
    if verdict.stderr_empty_matched == Some(false) {
        failures.push(format!("Command '{command}': stderr was not empty"));
    }
    failures
}

fn check_exit(
    contract: &CriterionContract,
    result: &CriterionResult,
    command: &str,
    verdict: &AssertionVerdict,
    failures: &mut Vec<String>,
) {
    if !verdict.exit_code_matched {
        let actual = result
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "no exit code".to_string());
        failures.push(format!(
            "Command '{command}': expected exit code {}, got {actual}",
            contract.expected_exit()
        ));
    }
}

fn check_stdout(
    contract: &CriterionContract,
    command: &str,
    verdict: &AssertionVerdict,
    failures: &mut Vec<String>,
) {
    for (index, pattern) in contract.stdout_contains().iter().enumerate() {
        if verdict.stdout_contains_matched.get(index) != Some(&true) {
            failures.push(format!(
                "Command '{command}': stdout missing expected pattern '{pattern}'"
            ));
        }
    }
    for (index, pattern) in contract.stdout_not_contains().iter().enumerate() {
        if verdict.stdout_not_contains_matched.get(index) != Some(&true) {
            failures.push(format!(
                "Command '{command}': stdout contains forbidden pattern '{pattern}'"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{CommandConfinement, TruthCheck};
    use crate::verify::criteria::confine::CommandSpec;

    #[test]
    fn missing_exit_never_matches_expected_negative_one() {
        let criterion = AcceptanceCriterion::Extended(TruthCheck {
            command: "probe".to_string(),
            stdout_contains: Vec::new(),
            stdout_not_contains: Vec::new(),
            stderr_empty: None,
            exit_code: Some(-1),
            description: None,
        });
        let contract = CriterionContract::new(
            &CommandSpec::shell("probe"),
            &criterion,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        );
        let result = CriterionResult::new(
            "probe".to_string(),
            false,
            String::new(),
            String::new(),
            None,
            Duration::ZERO,
            false,
        );
        let verdict = contract.verdict(&result);
        let command = CriterionCommand {
            written: "probe",
            expanded: "probe",
            cwd: Path::new("."),
        };

        let failures = check_criterion(&criterion, &contract, &result, &command, &verdict);

        assert!(!verdict.passed && !verdict.exit_code_matched);
        assert_eq!(
            failures,
            ["Command 'probe': expected exit code -1, got no exit code"]
        );
    }
}
