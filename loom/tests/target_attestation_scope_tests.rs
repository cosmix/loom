//! `core.hooksPath` set in git's global or system config makes git skip loom's
//! `reference-transaction` hook, so `loom target status` must report
//! attestation off. Each case runs the built binary with `GIT_CONFIG_GLOBAL`
//! and `GIT_CONFIG_SYSTEM` in the child's environment only: set in this
//! process, they would turn attestation off for every test running beside it.

// bespoke-git-fixture: the subject is how `loom target status` reads GIT_CONFIG_GLOBAL, GIT_CONFIG_SYSTEM and GIT_CONFIG_NOSYSTEM in the child environment; the shared runner would strip or fix those variables.
#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "target_ref_hook_support/mod.rs"]
#[allow(dead_code)]
mod support;
// the support module reaches the shared git module as `crate::test_support`
use helpers::test_support;

use std::path::{Path, PathBuf};
use support::{repo, Repo};

/// `loom target status` from the root of `repo`, with git's global and system
/// config files at `global` and `system` (a missing file is an empty one) and
/// the switch that skips the system file removed.
fn target_status(repo: &Repo, global: &Path, system: &Path) -> String {
    let out = helpers::loom_cmd()
        .args(["target", "status"])
        .current_dir(&repo.root)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_SYSTEM", system)
        .env_remove("GIT_CONFIG_NOSYSTEM")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A git config file in `repo`'s root that sets `core.hooksPath`.
fn hooks_path_config(repo: &Repo, name: &str, hooks_path: &str) -> PathBuf {
    let path = repo.root.join(name);
    std::fs::write(&path, format!("[core]\n\thooksPath = {hooks_path}\n")).unwrap();
    path
}

#[test]
fn without_a_hooks_path_in_any_scope_attestation_is_on() {
    let repo = repo();
    let global = repo.root.join("no-global-config");
    let system = repo.root.join("no-system-config");

    let status = target_status(&repo, &global, &system);

    assert!(status.contains("Attestation: on"), "{status}");
}

#[test]
fn hooks_path_in_global_config_turns_attestation_off_and_wins_over_system() {
    let repo = repo();
    let global = hooks_path_config(&repo, "global-config", "/global/hooks");
    let system = hooks_path_config(&repo, "system-config", "/system/hooks");

    let status = target_status(&repo, &global, &system);

    let expected = "Attestation: off (core.hooksPath is set to /global/hooks,";
    assert!(status.contains(expected), "{status}");
}

#[test]
fn hooks_path_in_system_config_turns_attestation_off() {
    let repo = repo();
    let global = repo.root.join("no-global-config");
    let system = hooks_path_config(&repo, "system-config", "/system/hooks");

    let status = target_status(&repo, &global, &system);

    let expected = "Attestation: off (core.hooksPath is set to /system/hooks,";
    assert!(status.contains(expected), "{status}");
}
