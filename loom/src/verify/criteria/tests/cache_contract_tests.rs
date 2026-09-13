//! Contract and certified-record regression tests.

use std::time::Duration;

use serial_test::serial;

use crate::models::stage::{AcceptanceCriterion, CommandConfinement, TruthCheck};
use crate::verify::criteria::cache_contract::{CachedCriterionPass, CriterionContract};
use crate::verify::criteria::cache_fingerprint::InputFingerprint;
use crate::verify::criteria::confine::CommandSpec;
use crate::verify::criteria::executor::OUTPUT_TRUNCATED_MARKER;
use crate::verify::criteria::result::CriterionResult;

fn extended(command: &str, contains: &[&str]) -> AcceptanceCriterion {
    AcceptanceCriterion::Extended(TruthCheck {
        command: command.to_string(),
        stdout_contains: contains.iter().map(|value| (*value).to_string()).collect(),
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: None,
        description: None,
    })
}

fn contract(
    spec: &CommandSpec,
    criterion: &AcceptanceCriterion,
    timeout: Duration,
    confinement: CommandConfinement,
) -> CriterionContract {
    CriterionContract::new(spec, criterion, timeout, confinement)
}

fn fingerprint() -> InputFingerprint {
    InputFingerprint {
        digest: "input-digest".to_string(),
        tree_head: "source-head".to_string(),
    }
}

fn result(exit: i32, stdout: &str, stderr: &str) -> CriterionResult {
    CriterionResult::new(
        "command".to_string(),
        exit == 0,
        stdout.to_string(),
        stderr.to_string(),
        Some(exit),
        Duration::from_secs(12),
        false,
    )
}

#[test]
#[serial]
fn contract_digest_covers_kind_and_output_patterns() {
    let spec = CommandSpec::shell("printf alpha");
    let simple = AcceptanceCriterion::Simple("printf alpha".to_string());
    let alpha = extended("printf alpha", &["alpha"]);
    let beta = extended("printf alpha", &["beta"]);
    let mut negative = alpha.clone();
    as_extended_mut(&mut negative)
        .stdout_not_contains
        .push("forbidden".to_string());
    let base = contract(
        &spec,
        &alpha,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let variants = [
        contract(
            &spec,
            &simple,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        ),
        contract(
            &spec,
            &beta,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        ),
        contract(
            &spec,
            &negative,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        ),
    ];

    assert_distinct(&base, &variants);
}

#[test]
#[serial]
fn contract_digest_covers_exit_stderr_timeout_and_confinement() {
    let spec = CommandSpec::shell("printf alpha");
    let alpha = extended("printf alpha", &["alpha"]);
    let mut stderr = alpha.clone();
    as_extended_mut(&mut stderr).stderr_empty = Some(true);
    let mut nonzero = alpha.clone();
    as_extended_mut(&mut nonzero).exit_code = Some(7);
    let variants = [
        contract(
            &spec,
            &stderr,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        ),
        contract(
            &spec,
            &nonzero,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        ),
        contract(
            &spec,
            &alpha,
            Duration::from_secs(2),
            CommandConfinement::Confined,
        ),
        contract(
            &spec,
            &alpha,
            Duration::from_secs(1),
            CommandConfinement::Inherit,
        ),
    ];
    let base = contract(
        &spec,
        &alpha,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );

    assert_distinct(&base, &variants);
}

fn assert_distinct(base: &CriterionContract, variants: &[CriterionContract]) {
    let digest = base.digest().unwrap();
    assert!(variants
        .iter()
        .all(|variant| variant.digest().unwrap() != digest));
}

fn as_extended_mut(criterion: &mut AcceptanceCriterion) -> &mut TruthCheck {
    match criterion {
        AcceptanceCriterion::Extended(check) => check,
        AcceptanceCriterion::Simple(_) => unreachable!(),
    }
}

#[test]
#[serial]
fn contract_digest_is_lossless_for_setup_shell_and_program_argv() {
    let criterion = AcceptanceCriterion::Simple("run".to_string());
    let shell = contract(
        &CommandSpec::shell("setup && run"),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let shell_without_setup = contract(
        &CommandSpec::shell("run"),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let program_one_arg = contract(
        &CommandSpec::program("setup", ["&& run"]),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let program_two_args = contract(
        &CommandSpec::program("setup", ["&&", "run"]),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );

    assert_ne!(shell.digest(), shell_without_setup.digest());
    assert_ne!(shell.digest(), program_one_arg.digest());
    assert_ne!(program_one_arg.digest(), program_two_args.digest());
}

#[test]
fn certified_record_preserves_nonzero_exit_and_bounded_diagnostics() {
    let mut check = match extended("command", &["needle"]) {
        AcceptanceCriterion::Extended(check) => check,
        AcceptanceCriterion::Simple(_) => unreachable!(),
    };
    check.exit_code = Some(7);
    let criterion = AcceptanceCriterion::Extended(check);
    let contract = contract(
        &CommandSpec::shell("command"),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let output = format!("{}needle", "x".repeat(8 * 1024));
    let result = result(7, &output, "");
    let verdict = contract.verdict(&result);

    let record = CachedCriterionPass::from_result(
        &contract,
        &fingerprint(),
        &result,
        result.duration,
        verdict,
    )
    .unwrap();
    assert!(record.certifies(&contract, &fingerprint()));
    assert_eq!(record.actual_exit, 7);
    assert_eq!(record.original_duration_ms, 12_000);
    assert!(record.stdout_tail_truncated);
    assert_eq!(record.stdout_tail.len(), 4 * 1024);
}

#[test]
fn invalid_verdict_cannot_certify_a_pass() {
    let criterion = extended("printf ok", &["ok"]);
    let contract = contract(
        &CommandSpec::shell("printf ok"),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let result = result(0, "ok", "");
    let verdict = contract.verdict(&result);
    let mut record = CachedCriterionPass::from_result(
        &contract,
        &fingerprint(),
        &result,
        result.duration,
        verdict,
    )
    .unwrap();

    record.verdict.stdout_contains_matched[0] = false;
    assert!(!record.certifies(&contract, &fingerprint()));
}

#[test]
fn truncated_evaluation_output_is_never_publishable() {
    let criterion = extended("command", &["ok"]);
    let contract = contract(
        &CommandSpec::shell("command"),
        &criterion,
        Duration::from_secs(1),
        CommandConfinement::Confined,
    );
    let output = format!("ok\n{OUTPUT_TRUNCATED_MARKER}");
    let result = result(0, &output, "");
    let verdict = contract.verdict(&result);

    assert!(CachedCriterionPass::from_result(
        &contract,
        &fingerprint(),
        &result,
        result.duration,
        verdict,
    )
    .is_none());
}

#[test]
fn cached_result_reports_current_lookup_duration() {
    let lookup = Duration::from_millis(3);
    let result = CriterionResult::cached_verdict(
        "command".to_string(),
        Some(7),
        "diagnostic".to_string(),
        String::new(),
        lookup,
    );

    assert_eq!(result.duration, lookup);
    assert_eq!(result.exit_code, Some(7));
    assert!(result.cached && result.success);
}
