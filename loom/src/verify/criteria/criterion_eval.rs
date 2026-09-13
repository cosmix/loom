//! Full evaluation of simple and extended acceptance assertions.

use std::time::Duration;

use super::cache_contract::{AssertionVerdict, CriterionContract};
use super::result::CriterionResult;
use crate::models::stage::AcceptanceCriterion;

pub(super) fn check_criterion(
    criterion: &AcceptanceCriterion,
    contract: &CriterionContract,
    result: &CriterionResult,
    command: &str,
    verdict: &AssertionVerdict,
) -> Vec<String> {
    match criterion {
        AcceptanceCriterion::Simple(_) => {
            check_simple(result, command, contract.timeout(), verdict)
        }
        AcceptanceCriterion::Extended(_) => {
            check_extended_criterion(contract, result, command, verdict)
        }
    }
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

        let failures = check_criterion(&criterion, &contract, &result, "probe", &verdict);

        assert!(!verdict.passed && !verdict.exit_code_matched);
        assert_eq!(
            failures,
            ["Command 'probe': expected exit code -1, got no exit code"]
        );
    }
}
