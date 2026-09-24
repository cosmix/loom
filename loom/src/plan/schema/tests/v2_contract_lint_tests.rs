//! Test-runner lints of `plan verify` (DESIGN D4, stage `contract-phase`).

use std::path::Path;

use tempfile::TempDir;

use crate::git::run_git_checked;
use crate::plan::schema::validation::v2_fields::split_lint_findings;
use crate::plan::schema::validation::v2_lints::{run, LintContext, LintFinding};
use crate::plan::schema::{
    AcceptanceCriterion, ContractSpec, LoomConfig, LoomMetadata, StageDefinition, StageType,
};

fn contract(file: &str, runner: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: "greets-by-name".to_string(),
        file: file.to_string(),
        test: "greets_by_name".to_string(),
        runner: runner.map(str::to_string),
        scenario: "a greeting is built for Ada".to_string(),
        rejects: "a greeting that ignores the name".to_string(),
    }
}

fn stage(
    stage_type: StageType,
    acceptance: &[&str],
    contracts: Vec<ContractSpec>,
) -> StageDefinition {
    StageDefinition {
        id: "stage".to_string(),
        name: "stage".to_string(),
        working_dir: ".".to_string(),
        stage_type: Some(stage_type),
        acceptance: acceptance
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect(),
        contracts,
        ..Default::default()
    }
}

fn plan(stage: StageDefinition) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 2,
            stages: vec![stage],
            ..Default::default()
        },
    }
}

/// The findings for `metadata` whose message contains `needle`.
fn matching(metadata: &LoomMetadata, repo_root: Option<&Path>, needle: &str) -> Vec<LintFinding> {
    let ctx = LintContext {
        metadata,
        repo_root,
    };
    run(&ctx, &mut Vec::new())
        .into_iter()
        .filter(|finding| finding.message.contains(needle))
        .collect()
}

fn standard_with(contract: ContractSpec) -> LoomMetadata {
    plan(stage(StageType::Standard, &["cargo test"], vec![contract]))
}

#[test]
fn contract_with_unknown_runner_is_rejected() {
    let unknown = standard_with(contract("tests/greeting.rs", Some("cargo-tset")));
    let known = standard_with(contract("tests/greeting.rs", Some("cargo-test")));

    let findings = matching(&unknown, None, "names runner `cargo-tset`");

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].message.contains("cargo-test, "));
    let (errors, warnings) = split_lint_findings(2, findings);
    assert_eq!((errors.len(), warnings.len()), (1, 0));
    assert!(matching(&known, None, "names runner").is_empty());
}

#[test]
fn contract_runner_detection_follows_the_owning_package() {
    let repo = TempDir::new().expect("temp dir");
    run_git_checked(&["init", "-q"], repo.path()).expect("git init");
    std::fs::write(repo.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    std::fs::create_dir(repo.path().join("web")).unwrap();
    std::fs::write(repo.path().join("web/package.json"), "{}\n").unwrap();
    let rust = standard_with(contract("tests/greeting.rs", None));
    let bare_js = standard_with(contract("web/greeting.test.js", None));

    let undetected = matching(&bare_js, Some(repo.path()), "names no `runner`");

    assert_eq!(undetected.len(), 1, "{undetected:?}");
    assert!(!undetected[0].error_in_v2);
    assert!(undetected[0]
        .message
        .contains("falls back to the test command's exit code"));
    assert!(matching(&rust, Some(repo.path()), "names no `runner`").is_empty());
}

#[test]
fn v2_iv_without_full_test_command_is_rejected() {
    let needle = "no acceptance command that runs a whole test suite";
    let filtered = plan(stage(
        StageType::IntegrationVerify,
        &["cargo test --lib x::"],
        Vec::new(),
    ));
    let full = plan(stage(
        StageType::IntegrationVerify,
        &["cargo test --lib x::", "cargo test --all-targets"],
        Vec::new(),
    ));

    let findings = matching(&filtered, None, needle);

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].error_in_v2);
    assert!(matching(&full, None, needle).is_empty());
}

#[test]
fn iv_full_test_lint_skips_v1_plans() {
    let mut filtered = plan(stage(
        StageType::IntegrationVerify,
        &["cargo test --lib x::"],
        Vec::new(),
    ));
    filtered.loom.version = 1;

    let findings = matching(&filtered, None, "runs a whole test suite");

    assert!(findings.is_empty(), "{findings:?}");
}
