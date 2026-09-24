//! Behavioural contracts of a v2 `standard` stage (DESIGN D8, D9).
//!
//! A contract session writes the contract tests first and freezes them while
//! they fail; the stage session then implements until they pass. [`store`]
//! keeps the freeze, [`changes`] reads what the contract session changed,
//! [`completion`] checks the frozen contracts when the stage completes. The
//! helpers here are what the freeze CLI, the daemon's freeze handler and the
//! completion check share, so the three agree on which command runs a
//! contract and which paths a contract session may touch.

pub mod changes;
pub mod completion;
pub mod site;
pub mod store;
#[cfg(test)]
pub(crate) mod test_support;

use glob::{MatchOptions, Pattern};
use std::path::{Component, Path};

use crate::plan::schema::ContractSpec;
use crate::skills::project::{PackageDetail, ProjectProfile};
use crate::testrun::{registry, TestRunnerAdapter};

/// The outcomes a contract may be frozen with: it has to fail first.
pub const RED_OUTCOMES: [&str; 3] = ["failed", "build_failed", "exit_nonzero_unverified"];

const GLOB_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// `path` with `.` components dropped, `/`-joined: the form paths are
/// compared, frozen and reported in.
pub fn normalize(path: &str) -> String {
    Path::new(path)
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `path` (relative to the working directory) matches a `harness` glob.
pub fn harness_matches(path: &str, harness: &[String]) -> bool {
    harness.iter().any(|glob| {
        Pattern::new(glob).is_ok_and(|pattern| pattern.matches_with(path, GLOB_OPTIONS))
    })
}

/// Whether a contract session may change `path` (relative to the working
/// directory): a contract `file` or a `harness` match. A path leaving the
/// working directory never qualifies.
pub fn is_contract_or_harness(path: &str, contracts: &[ContractSpec], harness: &[String]) -> bool {
    store::validate_relative(path).is_ok()
        && (contracts
            .iter()
            .any(|contract| normalize(&contract.file) == path)
            || harness_matches(path, harness))
}

/// The adapter that runs `contract`: its `runner` by name, else the runner
/// DESIGN D7 detects for the package owning its `file`. `package_dir` is the
/// stage's working directory, which `file` is relative to.
pub fn resolve_adapter(
    contract: &ContractSpec,
    package_dir: &Path,
) -> Option<&'static dyn TestRunnerAdapter> {
    match contract.runner.as_deref() {
        Some(name) => registry::by_name(name),
        None => detected_runner(package_dir, &contract.file).and_then(registry::by_name),
    }
}

fn detected_runner(package_dir: &Path, file: &str) -> Option<&'static str> {
    let profile = ProjectProfile::discover(package_dir);
    let target = package_dir.canonicalize().ok()?.join(normalize(file));
    let owned = target.strip_prefix(&profile.root).ok()?;
    let packages = profile.package_details();
    owning_package(&packages, owned).and_then(|package| package.runner)
}

/// The innermost package containing `path` (relative to the checkout root).
pub(in crate::verify) fn owning_package<'p>(
    packages: &'p [PackageDetail],
    path: &Path,
) -> Option<&'p PackageDetail> {
    packages
        .iter()
        .filter(|package| path.starts_with(&package.path))
        .max_by_key(|package| package.path.components().count())
}

/// The shell command that runs `contract` from `package_dir`: the adapter's
/// single-test command, or the contract's `test` itself when no adapter
/// applies, in which case only its exit code can be judged.
pub fn contract_command(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    package_dir: &Path,
) -> String {
    match adapter {
        Some(adapter) => {
            adapter.single_test_command(&normalize(&contract.file), &contract.test, package_dir)
        }
        None => contract.test.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(file: &str) -> ContractSpec {
        ContractSpec {
            id: "rejects-x".to_string(),
            file: file.to_string(),
            test: "tests::rejects_x".to_string(),
            runner: Some("cargo-test".to_string()),
            scenario: "s".to_string(),
            rejects: "r".to_string(),
        }
    }

    #[test]
    fn only_contract_files_and_harness_matches_qualify() {
        let contracts = [contract("./tests/x_contract.rs")];
        let harness = ["tests/fixtures/**".to_string(), "tests/*.json".to_string()];
        assert!(is_contract_or_harness(
            "tests/x_contract.rs",
            &contracts,
            &harness
        ));
        assert!(is_contract_or_harness(
            "tests/fixtures/a/b.txt",
            &contracts,
            &harness
        ));
        assert!(is_contract_or_harness(
            "tests/data.json",
            &contracts,
            &harness
        ));
        assert!(!is_contract_or_harness(
            "tests/sub/data.json",
            &contracts,
            &harness
        ));
        assert!(!is_contract_or_harness("src/lib.rs", &contracts, &harness));
        assert!(!is_contract_or_harness(
            "../tests/fixtures/a",
            &contracts,
            &harness
        ));
    }

    #[test]
    fn a_named_runner_builds_its_single_test_command() {
        let contract = contract("tests/x_contract.rs");
        let adapter = resolve_adapter(&contract, Path::new(".")).unwrap();
        assert_eq!(adapter.name(), "cargo-test");
        let command = contract_command(&contract, Some(adapter), Path::new("."));
        assert!(command.contains("tests::rejects_x"), "{command}");
        assert_eq!(
            contract_command(&contract, None, Path::new(".")),
            contract.test
        );
    }
}
