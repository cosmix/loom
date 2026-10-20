//! Repository-backed environment lints: the pre-commit hook the stages run
//! when they commit, and JS packages that need a `provision` entry.

use std::path::Path;

use tempfile::TempDir;

use crate::plan::schema::tests::create_valid_metadata_v2;
use crate::plan::schema::{validate, LoomMetadata, ProvisionEntry};

use super::{
    git_repo, isolated_git, lint, messages, network, plan, plan_allowing, stage, write, LintFinding,
};

const HOOK_MESSAGE: &str = "the repository's pre-commit hook";
const HOOK: &str = "#!/bin/sh\n\
    MD_OUT=$(git ls-files -z -- '*.md' | xargs -0 bunx markdownlint-cli2 --fix 2>&1) || true\n";
const VITEST_PACKAGE: &str = r#"{"devDependencies":{"vitest":"^3.2.0"}}"#;
const BUN_INSTALL: &str = "test ! -e .npmrc && test ! -L .npmrc && bun install \
                           --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null";
const NPM_INSTALL: &str = "test ! -e .npmrc && test ! -L .npmrc && npm ci --ignore-scripts";
/// A file in `web/`, and the install the lint suggests for it: each lockfile kind the
/// lint knows, and a file that is no lockfile.
const SUGGESTED_INSTALLS: [(&str, &str); 6] = [
    ("bun.lock", BUN_INSTALL),
    ("bun.lockb", BUN_INSTALL),
    (
        "pnpm-lock.yaml",
        "test ! -e .npmrc && test ! -L .npmrc && pnpm install --frozen-lockfile \
         --ignore-scripts --ignore-pnpmfile",
    ),
    (
        "yarn.lock",
        "test ! -e .npmrc && test ! -L .npmrc && yarn install --frozen-lockfile \
         --ignore-scripts",
    ),
    ("package-lock.json", NPM_INSTALL),
    ("README.md", NPM_INSTALL),
];

/// A repository whose LOCAL `core.hooksPath` is `hooks_path`: the local scope wins over
/// the machine's global and system config, so the lint's own reads stay deterministic.
fn hook_repo(hooks_path: &str) -> TempDir {
    let repo = git_repo();
    isolated_git(&["config", "core.hooksPath", hooks_path], repo.path());
    repo
}

fn hook_findings(repo: &Path, domains: &[&str]) -> Vec<String> {
    messages(&plan_allowing(domains, &[]), Some(repo), HOOK_MESSAGE)
}

#[test]
fn relative_hooks_path_is_read_from_the_local_scope() {
    let repo = hook_repo("hooks");
    write(repo.path(), "hooks/pre-commit", HOOK);
    let found = hook_findings(repo.path(), &["crates.io"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("`hooks/pre-commit` runs `bunx`"),
        "{found:?}"
    );
    assert!(found[0].contains("needs registry.npmjs.org"), "{found:?}");
    assert!(found[0].contains("when the stage commits"), "{found:?}");
}

#[test]
fn absolute_hooks_path_is_read_in_place() {
    let hooks = TempDir::new().expect("hooks dir");
    write(hooks.path(), "pre-commit", HOOK);
    let repo = hook_repo(&hooks.path().to_string_lossy());
    let found = hook_findings(repo.path(), &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(&hooks.path().join("pre-commit").display().to_string()));
}

#[test]
fn a_repository_without_a_hook_has_no_hook_finding() {
    let repo = hook_repo("hooks");
    assert!(hook_findings(repo.path(), &[]).is_empty());
    let empty = hook_repo("empty-hooks");
    std::fs::create_dir_all(empty.path().join("empty-hooks")).expect("empty hooks dir");
    assert!(hook_findings(empty.path(), &[]).is_empty());
    write(
        repo.path(),
        "hooks/pre-commit",
        "#!/bin/sh\ncargo fmt --check\n",
    );
    assert!(hook_findings(repo.path(), &[]).is_empty());
}

#[test]
fn hook_finding_is_per_stage_and_respects_each_sandbox() {
    let repo = hook_repo("hooks");
    write(repo.path(), "hooks/pre-commit", HOOK);
    let mut metadata = plan(
        2,
        vec![stage("one", &[]), stage("two", &[]), stage("off", &[])],
    );
    metadata.loom.stages[1].sandbox.network = Some(network(&["registry.npmjs.org"]));
    metadata.loom.stages[2].sandbox.enabled = Some(false);
    let (findings, _) = lint(&metadata, Some(repo.path()));
    let hook: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains(HOOK_MESSAGE))
        .collect();
    assert_eq!(hook.len(), 1, "{findings:?}");
    assert_eq!(hook[0].stage_id.as_deref(), Some("one"));
    assert!(hook[0].error_in_v2);
    assert!(messages(&metadata, None, HOOK_MESSAGE).is_empty());
}

fn js_repo(files: &[(&str, &str)]) -> TempDir {
    let repo = git_repo();
    write(repo.path(), "web/package.json", VITEST_PACKAGE);
    for (path, body) in files {
        write(repo.path(), path, body);
    }
    repo
}

fn with_provision(version: u32, dirs: &[&str]) -> LoomMetadata {
    let mut metadata = plan(version, vec![stage("feature", &[])]);
    metadata.loom.provision = dirs
        .iter()
        .map(|dir| ProvisionEntry {
            working_dir: dir.to_string(),
            command: "true".to_string(),
        })
        .collect();
    metadata
}

fn package_findings(metadata: &LoomMetadata, repo: &Path) -> Vec<LintFinding> {
    let (findings, _) = lint(metadata, Some(repo));
    findings
        .into_iter()
        .filter(|finding| finding.message.starts_with("package `"))
        .collect()
}

#[test]
fn uncovered_js_package_names_its_install_command() {
    for (file, install) in SUGGESTED_INSTALLS {
        let repo = js_repo(&[(&format!("web/{file}"), "")]);
        let found = package_findings(&with_provision(2, &[]), repo.path());
        assert_eq!(found.len(), 1, "{file}: {found:?}");
        assert_eq!(found[0].stage_id, None);
        assert!(found[0].error_in_v2);
        assert!(found[0]
            .message
            .starts_with("package `web` runs its tests with "));
        let entry = format!("{{ working_dir: \"web\", command: \"{install}\" }}");
        assert!(found[0].message.contains(&entry), "{file}: {found:?}");
    }
}

/// The finding's suggested command must pass provision validation, so a pasted
/// suggestion never trades the lint error for a validation error.
#[test]
fn every_suggested_install_passes_provision_validation() {
    for (file, _) in SUGGESTED_INSTALLS {
        let repo = js_repo(&[(&format!("web/{file}"), "")]);
        let found = package_findings(&with_provision(2, &[]), repo.path());
        assert_eq!(found.len(), 1, "{file}: {found:?}");
        let suggested = found[0]
            .message
            .split("command: \"")
            .nth(1)
            .and_then(|rest| rest.split("\" }").next())
            .expect("the finding suggests a command");
        let mut metadata = create_valid_metadata_v2();
        metadata.loom.provision = vec![ProvisionEntry {
            working_dir: "web".to_string(),
            command: suggested.to_string(),
        }];
        let errors = validate(&metadata).err().unwrap_or_default();
        assert!(errors.is_empty(), "{file}: {suggested}: {errors:?}");
    }
}

#[test]
fn a_package_without_a_lockfile_is_told_to_commit_one() {
    let repo = js_repo(&[]);
    let found = package_findings(&with_provision(2, &[]), repo.path());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.contains("commit a lockfile first"),
        "{found:?}"
    );
}

#[test]
fn a_package_without_dependencies_needs_no_provision() {
    let repo = git_repo();
    write(
        repo.path(),
        "web/package.json",
        r#"{"scripts":{"test":"node --test"}}"#,
    );
    let found = package_findings(&with_provision(2, &[]), repo.path());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn provision_entries_cover_their_package_and_ancestors() {
    let repo = js_repo(&[]);
    for dirs in [&["web"][..], &["."], &["./web"], &["web/"]] {
        let found = package_findings(&with_provision(2, dirs), repo.path());
        assert!(found.is_empty(), "{dirs:?}: {found:?}");
    }
    write(repo.path(), "web/sub/keep", "");
    let found = package_findings(&with_provision(2, &["web/sub"]), repo.path());
    assert_eq!(
        found.len(),
        1,
        "an entry below the package does not cover it: {found:?}"
    );
}

#[test]
fn provision_entry_for_a_missing_directory_is_an_error() {
    let repo = js_repo(&[]);
    let metadata = with_provision(2, &["web", "nowhere"]);
    let found = messages(&metadata, Some(repo.path()), "provision entry #");
    assert_eq!(
        found,
        ["provision entry #2 working_dir `nowhere` does not exist in the repository"]
    );
    let (findings, _) = lint(&metadata, Some(repo.path()));
    assert!(findings.iter().all(|finding| finding.error_in_v2));
}

/// Findings of the provision lints.
fn provision_findings(findings: &[LintFinding]) -> Vec<&LintFinding> {
    findings
        .iter()
        .filter(|finding| {
            finding.message.contains("provision") || finding.message.starts_with("package `")
        })
        .collect()
}

#[test]
fn version_one_plans_and_missing_repositories_have_no_provision_findings() {
    let repo = js_repo(&[]);
    let (findings, notes) = lint(&with_provision(1, &[]), Some(repo.path()));
    assert!(provision_findings(&findings).is_empty(), "{findings:?}");
    assert!(notes.is_empty(), "{notes:?}");
    let (findings, notes) = lint(&with_provision(2, &[]), None);
    assert!(provision_findings(&findings).is_empty(), "{findings:?}");
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_package_that_does_not_run_javascript_needs_no_provision() {
    let repo = git_repo();
    write(repo.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
    let found = package_findings(&with_provision(2, &[]), repo.path());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_truncated_package_scan_leaves_a_note() {
    let repo = js_repo(&[("a/b/c/d/e/f/g/h/i/j/Cargo.toml", "[package]")]);
    let (_, notes) = lint(&with_provision(2, &["web"]), Some(repo.path()));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("package scan stopped"), "{notes:?}");
}
