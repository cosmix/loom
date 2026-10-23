//! Collection lints over HEAD of a temporary repository holding a binary
//! package.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::super::{run, LintContext, LintFinding};
use crate::plan::schema::{ContractSpec, LoomConfig, LoomMetadata, StageDefinition};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository whose one commit holds a binary package at its root.
fn repo() -> TempDir {
    repo_in("")
}

/// A repository whose one commit holds a binary package in `dir` ("" is the
/// repository root) and, outside it, `tools/gen.rs`.
fn repo_in(dir: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    let package = root.join(dir);
    std::fs::create_dir_all(package.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::write(
        package.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(package.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("tools/gen.rs"), "fn main() {}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    temp
}

fn contract(file: &str, test: &str) -> ContractSpec {
    ContractSpec {
        id: "locks".to_string(),
        file: file.to_string(),
        test: test.to_string(),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "a lock is taken".to_string(),
        rejects: "a lock that is never taken".to_string(),
    }
}

fn stage(contract: ContractSpec, harness: &[&str], dependencies: &[&str]) -> StageDefinition {
    stage_in(".", contract, harness, dependencies)
}

fn stage_in(
    working_dir: &str,
    contract: ContractSpec,
    harness: &[&str],
    dependencies: &[&str],
) -> StageDefinition {
    StageDefinition {
        id: "stage".to_string(),
        name: "stage".to_string(),
        working_dir: working_dir.to_string(),
        contracts: vec![contract],
        harness: harness.iter().map(|glob| glob.to_string()).collect(),
        dependencies: dependencies.iter().map(|id| id.to_string()).collect(),
        ..Default::default()
    }
}

/// The collection findings (`contract <id>:` and harness) for `stage`.
fn findings(root: &Path, stage: StageDefinition) -> Vec<LintFinding> {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
            stages: vec![stage],
            ..Default::default()
        },
    };
    let ctx = LintContext {
        metadata: &metadata,
        repo_root: Some(root),
    };
    run(&ctx, &mut Vec::new())
        .into_iter()
        .filter(|f| f.message.starts_with("contract locks:") || f.message.contains("harness glob"))
        .collect()
}

const LIQUIDMON: &str = "src/device_lock/contract_tests.rs";
const LIQUIDMON_TEST: &str = "device_lock::contract_tests::rejects_x";

#[test]
fn a_contract_in_a_new_module_derives_a_scaffold_without_error() {
    let repo = repo();

    let found = findings(
        repo.path(),
        stage(contract(LIQUIDMON, LIQUIDMON_TEST), &[], &[]),
    );

    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_new_integration_test_root_needs_no_scaffold() {
    let repo = repo();

    let found = findings(
        repo.path(),
        stage(contract("tests/x_contracts.rs", "x::y"), &[], &[]),
    );

    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_harness_glob_over_production_source_warns() {
    let repo = repo();

    let found = findings(
        repo.path(),
        stage(contract(LIQUIDMON, LIQUIDMON_TEST), &["src/main.rs"], &[]),
    );

    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .message
            .contains("harness glob matches production source src/main.rs"),
        "{found:?}"
    );
    assert!(!found[0].error_in_v2);
}

#[test]
fn a_contract_no_target_can_compile_is_an_error_without_dependencies() {
    let repo = repo();
    let nowhere = contract("src/nowhere/contract_tests.rs", "elsewhere::x");

    let alone = findings(repo.path(), stage(nowhere.clone(), &[], &[]));
    let later = findings(repo.path(), stage(nowhere, &[], &["package"]));

    assert_eq!(alone.len(), 1, "{alone:?}");
    assert!(alone[0].error_in_v2, "{alone:?}");
    assert_eq!(later.len(), 1, "{later:?}");
    assert!(!later[0].error_in_v2, "{later:?}");
}

/// The lints read HEAD: an unparsable uncommitted `src/main.rs` and an
/// untracked module root change nothing.
#[test]
fn the_lints_read_head_not_the_checkout() {
    let repo = repo();
    let root = repo.path();
    std::fs::write(root.join("src/main.rs"), "fn ((( {\n").unwrap();
    std::fs::create_dir_all(root.join("src/device_lock")).unwrap();
    std::fs::write(root.join("src/device_lock/mod.rs"), "fn ((( {\n").unwrap();

    let found = findings(root, stage(contract(LIQUIDMON, LIQUIDMON_TEST), &[], &[]));

    assert!(found.is_empty(), "{found:?}");
}

/// The package lives in `pkg/`, the stage's working directory: contract and
/// harness paths are relative to it, HEAD paths to the repository root.
mod in_a_subdirectory {
    use super::*;

    #[test]
    fn a_contract_in_a_new_module_derives_a_scaffold_without_error() {
        let repo = repo_in("pkg");

        let found = findings(
            repo.path(),
            stage_in("pkg", contract(LIQUIDMON, LIQUIDMON_TEST), &[], &[]),
        );

        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_harness_glob_over_production_source_warns() {
        let repo = repo_in("pkg");

        let found = findings(
            repo.path(),
            stage_in(
                "pkg",
                contract(LIQUIDMON, LIQUIDMON_TEST),
                &["src/main.rs"],
                &[],
            ),
        );

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0]
                .message
                .contains("harness glob matches production source src/main.rs"),
            "{found:?}"
        );
        assert!(!found[0].error_in_v2);
    }

    #[test]
    fn a_harness_glob_matches_only_files_under_the_working_directory() {
        let repo = repo_in("pkg");

        let found = findings(
            repo.path(),
            stage_in(
                "pkg",
                contract(LIQUIDMON, LIQUIDMON_TEST),
                &["tools/gen.rs", "pkg/src/main.rs"],
                &[],
            ),
        );

        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_contract_no_target_can_compile_is_an_error_naming_the_package_file() {
        let repo = repo_in("pkg");
        let nowhere = contract("src/nowhere/contract_tests.rs", "elsewhere::x");

        let found = findings(repo.path(), stage_in("pkg", nowhere, &[], &[]));

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].error_in_v2, "{found:?}");
        assert!(!found[0].message.contains("no Cargo package"), "{found:?}");
    }
}
