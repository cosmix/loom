//! JavaScript packages that declare dependencies need a plan `provision` entry:
//! a stage worktree is a fresh checkout with no `node_modules`, so
//! impact-selected tests and integration-verify cannot run the package's tests
//! until something installs them. Plan `version: 2` only, because `provision`
//! is v2-only.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::git::run_git_checked;
use crate::skills::project::{declares_dependencies, PackageDetail, ProjectProfile};
use crate::testrun::registry::by_name;

use super::super::v2_fields::{BUN_INSTALL, NPM_INSTALL, PNPM_INSTALL, YARN_INSTALL};
use super::{LintContext, LintFinding};

/// Lockfile name and the hardened install that honours it, first match wins. Each
/// is a form provision validation accepts, so a pasted suggestion verifies.
const INSTALLS: [(&str, &str); 5] = [
    ("bun.lock", BUN_INSTALL),
    ("bun.lockb", BUN_INSTALL),
    ("pnpm-lock.yaml", PNPM_INSTALL),
    ("yarn.lock", YARN_INSTALL),
    ("package-lock.json", NPM_INSTALL),
];

const TRUNCATED_NOTE: &str = "the package scan stopped at its depth or entry limit, so a JS \
                              package may be missing from the provision check";

/// Push the findings and return the notes.
pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) -> Vec<String> {
    let Some(root) = ctx.repo_root.filter(|_| ctx.metadata.loom.version == 2) else {
        return Vec::new();
    };
    let provision = &ctx.metadata.loom.provision;
    for (index, entry) in provision.iter().enumerate() {
        if !root.join(&entry.working_dir).is_dir() {
            out.push(plan_finding(format!(
                "provision entry #{} working_dir `{}` does not exist in the repository",
                index + 1,
                entry.working_dir
            )));
        }
    }
    let profile = ProjectProfile::discover(root);
    let covers = |package: &Path| {
        provision.iter().any(|entry| {
            let dir = normalized(Path::new(&entry.working_dir));
            normalized(package).starts_with(dir)
        })
    };
    let tracked = tracked_manifests(root);
    for package in profile.package_details() {
        let Some(runner) = js_runner(&package) else {
            continue;
        };
        let manifest = package.path.join("package.json");
        if tracked.as_ref().is_some_and(|set| !set.contains(&manifest)) {
            continue;
        }
        let dir = profile.root.join(&package.path);
        // A package with no dependencies runs its tests without an install.
        if !covers(&package.path) && declares_dependencies(&profile.root, &dir) {
            let install = install_command(&dir);
            out.push(plan_finding(uncovered_message(&package, runner, install)));
        }
    }
    if profile.truncated {
        vec![TRUNCATED_NOTE.to_string()]
    } else {
        Vec::new()
    }
}

fn plan_finding(message: String) -> LintFinding {
    LintFinding {
        stage_id: None,
        message,
        error_in_v2: true,
    }
}

/// The `package.json` files git tracks, `None` when git cannot list them. A stage
/// worktree is a checkout, so a package only the operator's tree holds (a tool cache
/// in an untracked directory) never reaches one and needs no provision entry.
fn tracked_manifests(root: &Path) -> Option<BTreeSet<PathBuf>> {
    let listed = run_git_checked(&["ls-files", "-z", "--", ":(glob)**/package.json"], root).ok()?;
    Some(
        listed
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

/// The package's test-runner adapter name when it runs JavaScript tests.
fn js_runner(package: &PackageDetail) -> Option<&'static str> {
    let runner = package.runner?;
    let is_js = by_name(runner).is_some_and(|adapter| adapter.language() == "javascript");
    is_js.then_some(runner)
}

/// The finding for a package no entry covers; `install` is `None` when the package
/// has no lockfile.
fn uncovered_message(package: &PackageDetail, runner: &str, install: Option<&str>) -> String {
    let path = package.path.display();
    // An unlocked install writes an untracked lockfile, which the provision gate
    // blocks as a file git does not ignore.
    let (step, install) = match install {
        Some(install) => ("", install),
        None => (
            "it has no lockfile, and provision installs only from a committed one: commit a \
             lockfile first (`npm install --package-lock-only --ignore-scripts` writes one), \
             then ",
            NPM_INSTALL,
        ),
    };
    format!(
        "package `{path}` runs its tests with {runner} and no `provision` entry covers it: \
         impact-selected tests and integration-verify cannot run it in a worktree, which has \
         no node_modules; {step}add `{{ working_dir: \"{path}\", command: \"{install}\" }}` to \
         `loom.provision`"
    )
}

/// The hardened install that matches the lockfile in `package_dir`, if it has one.
fn install_command(package_dir: &Path) -> Option<&'static str> {
    INSTALLS
        .iter()
        .find(|(lockfile, _)| package_dir.join(lockfile).exists())
        .map(|(_, command)| *command)
}

/// `path` without its `.` components, so `.` and `""` both name the root and
/// `./web` equals `web`.
fn normalized(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| *component != Component::CurDir)
        .collect()
}
