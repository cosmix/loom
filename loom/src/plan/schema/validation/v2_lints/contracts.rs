//! Test-runner lints (DESIGN D4, stage `contract-phase`): a contract naming a
//! runner no adapter answers to, a contract whose runner cannot be detected,
//! and a v2 integration-verify stage that never runs a whole test suite.

use std::path::{Path, PathBuf};

use crate::plan::schema::{detect_stage_type, ContractSpec, StageDefinition, StageType};
use crate::testrun::recognize::invocations;
use crate::testrun::registry;
use crate::verify::contracts::resolve_adapter;

use super::{LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    for stage in &ctx.metadata.loom.stages {
        for contract in &stage.contracts {
            match (&contract.runner, ctx.repo_root) {
                (Some(runner), _) => check_known_runner(stage, contract, runner, out),
                (None, Some(root)) => {
                    let package_dir = root.join(&stage.working_dir);
                    check_detected_runner(stage, contract, &package_dir, out);
                }
                (None, None) => {}
            }
        }
        if ctx.metadata.loom.version == 2 {
            check_full_test_run(stage, ctx.repo_root, out);
        }
    }
}

fn check_known_runner(
    stage: &StageDefinition,
    contract: &ContractSpec,
    runner: &str,
    out: &mut Vec<LintFinding>,
) {
    if registry::by_name(runner).is_some() {
        return;
    }
    let message = format!(
        "Contract `{}` names runner `{runner}`, which no test-runner adapter answers to; \
         use one of: {}",
        contract.id,
        adapter_names()
    );
    out.push(LintFinding::in_stage(stage, message, true));
}

/// The detection completion itself uses (`resolve_adapter`), so the warning
/// holds exactly when completion will judge the contract by exit code alone.
fn check_detected_runner(
    stage: &StageDefinition,
    contract: &ContractSpec,
    package_dir: &Path,
    out: &mut Vec<LintFinding>,
) {
    if resolve_adapter(contract, package_dir).is_some() {
        return;
    }
    let message = format!(
        "Contract `{}` names no `runner`, and detection finds no test-runner adapter for the \
         package owning `{}`: completion falls back to the test command's exit code; set \
         `runner` to one of: {}",
        contract.id,
        contract.file,
        adapter_names()
    );
    out.push(LintFinding::in_stage(stage, message, false));
}

/// G3: a v2 integration-verify stage must run some test suite unfiltered, so a
/// filter cannot narrow the final verification to a subset, or to nothing.
fn check_full_test_run(
    stage: &StageDefinition,
    repo_root: Option<&Path>,
    out: &mut Vec<LintFinding>,
) {
    if detect_stage_type(stage) != StageType::IntegrationVerify {
        return;
    }
    let cwd = repo_root.map_or_else(
        || PathBuf::from(&stage.working_dir),
        |root| root.join(&stage.working_dir),
    );
    let full = stage
        .acceptance
        .iter()
        .any(|criterion| runs_full_suite(criterion.command(), &cwd));
    if !full {
        let message = "Integration-verify stage has no acceptance command that runs a whole \
                       test suite (such as `cargo test --all-targets`); a filtered test command \
                       can select a subset of the tests, or none, and still pass"
            .to_string();
        out.push(LintFinding::in_stage(stage, message, true));
    }
}

/// Whether a simple command of `command` is a full run (`is_full_run`) of the
/// adapter that recognises it.
fn runs_full_suite(command: &str, cwd: &Path) -> bool {
    invocations(command, cwd).iter().any(|argv| {
        registry::all()
            .iter()
            .find(|adapter| adapter.recognizes(argv))
            .is_some_and(|adapter| adapter.is_full_run(argv))
    })
}

fn adapter_names() -> String {
    let names: Vec<&str> = registry::all()
        .iter()
        .map(|adapter| adapter.name())
        .collect();
    names.join(", ")
}
