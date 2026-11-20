//! The confined contract tests, launched: the OS sandbox enforces what
//! `contract_tests` reads off the generated policy. Each test skips where
//! confined commands cannot start (a sandboxed shell, a missing launcher).

use super::contract_tests::policy::{credential_fixture, seed_fixture, toolchain_fixture};
use super::contract_tests::{shown, stage, temp, HostEnvGuard};
use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::confined_commands_runnable;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{run_spec_in_stage, CommandSpec, CriterionResult};
use std::path::Path;
use std::time::Duration;

fn run_confined(script: &str, tree: &Path, cache: &StageCache) -> CriterionResult {
    let spec = CommandSpec::shell(script);
    let timeout = Duration::from_secs(60);
    let confined = CommandConfinement::Confined;
    run_spec_in_stage(&spec, Some(tree), Some(tree), timeout, confined, cache).unwrap()
}

#[test]
#[serial_test::serial]
fn stage_env_wins_over_per_command_scratch_in_the_kernel() {
    if !confined_commands_runnable() {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = stage(&repo, &home);
    let expected = "GOCACHE=go-build XDG_CACHE_HOME=xdg UV_CACHE_DIR=uv \
        BUN_INSTALL_CACHE_DIR=bun npm_config_cache=npm CCACHE_DIR=ccache \
        GRADLE_USER_HOME=gradle SCCACHE_DIR=sccache";
    let pairs: Vec<(&str, &str)> = expected
        .split_whitespace()
        .map(|pair| pair.split_once('=').unwrap())
        .collect();
    let script: Vec<String> = pairs
        .iter()
        .map(|(name, _)| format!("printf '{name}=%s\\n' \"${name}\""))
        .collect();
    let result = run_confined(&script.join("; "), &tree, &cache);
    assert!(result.success, "{result:?}");
    for (name, child) in pairs {
        let line = format!("{name}={}", shown(&cache.root().join(child)));
        let found = result.stdout.lines().any(|l| l == line);
        assert!(found, "{line} missing from\n{}", result.stdout);
    }
}

#[test]
#[serial_test::serial]
fn stage_cache_persists_across_confined_commands_in_the_kernel() {
    if !confined_commands_runnable() {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = stage(&repo, &home);
    let write = "mkdir -p \"$GOCACHE\" && printf persisted > \"$GOCACHE/probe\"";
    let first = run_confined(write, &tree, &cache);
    assert!(first.success, "the root must be writable inside: {first:?}");
    let probe = cache.root().join("go-build/probe");
    assert_eq!(std::fs::read_to_string(&probe).unwrap(), "persisted");
    let second = run_confined("cat \"$GOCACHE/probe\"", &tree, &cache.clone());
    assert!(second.success, "the root must survive commands: {second:?}");
    assert_eq!(second.stdout, "persisted");
}

#[test]
#[serial_test::serial]
fn host_caches_stay_read_only_in_the_kernel() {
    if !confined_commands_runnable() {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let (_seeds, dirs) = seed_fixture(&home);
    let script: Vec<String> = dirs
        .iter()
        .map(|dir| {
            let dir = shown(dir);
            format!("cat '{dir}/sentinel' || exit 10; printf x > '{dir}/written' && echo WROTE")
        })
        .collect();
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &[]);
    let result = run_confined(&format!("{}; echo DONE", script.join("; ")), &tree, &cache);
    assert!(result.success, "every seed must be readable: {result:?}");
    assert_eq!(result.stdout.matches("SENTINEL").count(), 3, "{result:?}");
    assert!(result.stdout.contains("DONE") && !result.stdout.contains("WROTE"));
    for dir in dirs {
        assert!(!dir.join("written").exists(), "{dir:?} was written");
    }
}

#[test]
#[serial_test::serial]
fn path_toolchains_are_readable_in_the_kernel() {
    if !confined_commands_runnable() {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let (_targets, entries) = toolchain_fixture(&home);
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &entries);
    let result = run_confined("faketool && othertool", &tree, &cache);
    assert!(result.success, "{result:?}");
    assert!(result.stdout.contains("TOOL_OK") && result.stdout.contains("OTHER_OK"));
}

#[test]
#[serial_test::serial]
fn credential_dirs_on_path_stay_hidden_in_the_kernel() {
    if !confined_commands_runnable() {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let entries = credential_fixture(&home);
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &entries);
    let marker = shown(&entries[2].join("marker"));
    let key = shown(&home.join(".ssh/key"));
    let result = run_confined(&format!("cat '{marker}'; cat '{key}'"), &tree, &cache);
    assert!(result.stdout.contains("MARKER_OK"), "control: {result:?}");
    assert!(!result.stdout.contains("SECRET_KEY"), "{result:?}");
    assert!(!result.success, "reading the key must fail: {result:?}");
}
