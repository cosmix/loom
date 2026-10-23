//! Behavioural contracts of a v2 `standard` stage (DESIGN D8, D9).
//!
//! A contract session writes the contract tests first and freezes them while
//! they fail; the stage session then implements until they pass. [`store`]
//! keeps the freeze, [`frozen_set`] reads the files a freeze or an accepted
//! contract dispute's re-freeze holds, [`changes`] reads what the contract
//! session changed,
//! [`refusal`] keeps a refused freeze until the writer stops and parks its
//! stage for the operator, [`completion`] checks the frozen contracts when the
//! stage completes, [`run_dir`] resolves where and how a contract runs. The
//! helpers here are what the freeze CLI, the daemon's freeze handler and the
//! completion check share, so the three agree on which adapter runs a contract
//! and which paths a contract session may touch. [`collection`] derives the
//! scaffold that puts a contract file into a test target and proves the
//! frozen file is the one compiled under the contract's test name.

pub mod changes;
pub mod collection;
pub mod completion;
pub mod format_gate;
pub mod frozen_set;
pub mod refusal;
pub mod run_dir;
pub mod site;
mod special_walk;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(file: &str) -> ContractSpec {
        ContractSpec {
            id: "rejects-x".to_string(),
            file: file.to_string(),
            test: "tests::rejects_x".to_string(),
            runner: Some("cargo-test".to_string()),
            cwd: None,
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
}
