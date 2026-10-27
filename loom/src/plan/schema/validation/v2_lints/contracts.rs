//! Test-runner lints (DESIGN D4, stage `contract-phase`): a contract naming a
//! runner no adapter answers to, a contract whose runner cannot be detected,
//! and a v2 integration-verify stage that never runs a whole test suite; and,
//! over HEAD of the repository, harness globs that match production source and
//! Rust contracts whose module chain cannot be derived (the scaffold).

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::super::base_tree::HeadTree;
use crate::plan::schema::{detect_stage_type, ContractSpec, StageDefinition, StageType};
use crate::skills::project::ProjectProfile;
use crate::testrun::recognize::invocations;
use crate::testrun::{registry, TestRunnerAdapter};
use crate::verify::contracts::collection::harness_source_matches;
use crate::verify::contracts::collection::rust::derive_scaffold;
use crate::verify::contracts::{detected_adapter, normalize};

use super::{LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let head = ctx.repo_root.and_then(HeadTree::new);
    let listing = OnceCell::new();
    let mut adapters = Adapters::default();
    for stage in &ctx.metadata.loom.stages {
        if let (Some(tree), Some(root)) = (&head, ctx.repo_root) {
            let head = HeadListing {
                tree,
                files: &listing,
            };
            check_collection(stage, &head, root, &mut adapters, out);
        }
        for contract in &stage.contracts {
            match (&contract.runner, ctx.repo_root) {
                (Some(runner), _) => check_known_runner(stage, contract, runner, out),
                (None, Some(root)) => {
                    let package_dir = root.join(&stage.working_dir);
                    check_detected_runner(stage, contract, &package_dir, &mut adapters, out);
                }
                (None, None) => {}
            }
        }
        if ctx.metadata.loom.version == 2 {
            check_full_test_run(stage, ctx.repo_root, out);
        }
    }
}

/// HEAD and its file listing, which every stage's collection lint reads; listed once per plan.
struct HeadListing<'h, 'r> {
    tree: &'h HeadTree<'r>,
    files: &'h OnceCell<Option<Vec<String>>>,
}

impl HeadListing<'_, '_> {
    /// Every tracked regular file at HEAD; `None` when git fails.
    fn files(&self) -> Option<&[String]> {
        self.files.get_or_init(|| self.tree.files("")).as_deref()
    }
}

/// `resolve_adapter`'s answers, with one checkout discovery per package directory for the whole
/// plan: discovery walks the checkout, and a plan names many contracts of one package.
#[derive(Default)]
struct Adapters(HashMap<PathBuf, ProjectProfile>);

impl Adapters {
    fn resolve(
        &mut self,
        contract: &ContractSpec,
        package_dir: &Path,
    ) -> Option<&'static dyn TestRunnerAdapter> {
        if let Some(name) = contract.runner.as_deref() {
            return registry::by_name(name);
        }
        let profile = self
            .0
            .entry(package_dir.to_path_buf())
            .or_insert_with(|| ProjectProfile::discover(package_dir));
        detected_adapter(profile, package_dir, &contract.file)
    }
}

/// The stage's directory relative to the repository root, `""` for the root.
fn working_prefix(stage: &StageDefinition) -> String {
    match normalize(&stage.working_dir).trim_matches('/') {
        "." => String::new(),
        dir => dir.to_string(),
    }
}

fn join(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

/// The HEAD files under the stage's directory `prefix`, relative to it.
fn files_under(listed: &[String], prefix: &str) -> Vec<String> {
    listed
        .iter()
        .filter_map(|file| {
            let relative = match prefix {
                "" => file.as_str(),
                _ => file.strip_prefix(prefix)?.strip_prefix('/')?,
            };
            Some(relative.to_string())
        })
        .collect()
}

/// Collection lints over HEAD, never the checkout: a harness glob that
/// matches production source, and a Rust contract whose module chain the
/// scaffold derivation rejects. A stage with `dependencies` may build the
/// package or module root the contract needs, so its derivation failure only
/// warns.
fn check_collection(
    stage: &StageDefinition,
    head: &HeadListing<'_, '_>,
    root: &Path,
    adapters: &mut Adapters,
    out: &mut Vec<LintFinding>,
) {
    // Listing HEAD is a git call; a stage with nothing to lint skips it.
    if stage.harness.is_empty() && stage.contracts.is_empty() {
        return;
    }
    let prefix = working_prefix(stage);
    let Some(listed) = head.files() else {
        return;
    };
    let tracked: HashSet<&str> = listed.iter().map(String::as_str).collect();
    for path in harness_source_matches(&stage.harness, &files_under(listed, &prefix)) {
        let message = format!(
            "harness glob matches production source {path}; it is frozen for the implementer. \
             Harness is for test-only files; module declarations are derived as scaffold"
        );
        out.push(LintFinding::in_stage(stage, message, false));
    }
    let source = |path: &str| {
        let bytes = tracked
            .contains(path)
            .then(|| head.tree.content(path))
            .flatten()?;
        String::from_utf8(bytes).ok()
    };
    let package_dir = root.join(&stage.working_dir);
    for contract in &stage.contracts {
        let adapter = adapters.resolve(contract, &package_dir);
        if !adapter.is_some_and(|adapter| adapter.language() == "rust") {
            continue;
        }
        let file = join(&prefix, &normalize(&contract.file));
        if let Err(error) = derive_scaffold(&source, &[(file.as_str(), contract.test.as_str())]) {
            let message = format!("contract {}: {error}", contract.id);
            let blocking = stage.dependencies.is_empty();
            out.push(LintFinding::in_stage(stage, message, blocking));
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
    adapters: &mut Adapters,
    out: &mut Vec<LintFinding>,
) {
    if adapters.resolve(contract, package_dir).is_some() {
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

#[cfg(test)]
#[path = "contracts_tests.rs"]
mod tests;
