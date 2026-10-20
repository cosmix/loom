//! Host-fact resolution: the recorded daemon binary, and the refusal a home
//! directory that does not resolve raises before a spawn or `loom run` goes
//! ahead.

use std::fs;

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;

#[test]
fn a_recorded_binary_wins_over_the_running_executable() {
    let dir = tempfile::tempdir().unwrap();
    let recorded = dir.path().join("loom");
    assert_eq!(binary_to_accept(Some(&recorded)).unwrap(), recorded);
}

#[test]
fn without_a_recording_the_running_executable_is_used() {
    assert_eq!(
        binary_to_accept(None).unwrap(),
        std::env::current_exe().unwrap()
    );
}

/// `error` is the preflight refusal naming the relative `HOME`.
fn assert_root_refusal(error: &anyhow::Error, case: &str) {
    let text = format!("{error:#}");
    assert!(error.is::<SandboxPreflightRefusal>(), "{case}: {text}");
    assert!(
        text.contains("install roots do not resolve")
            && text.contains("$HOME")
            && text.contains("must be absolute"),
        "{case}: {text}"
    );
}

/// A relative `HOME` refuses both a spawn and `loom run`, with or without
/// `LOOM_HOOKS_DIR`: neither may go ahead without the `~/.claude` and
/// `~/.codex` control surfaces resolved.
#[test]
#[serial]
fn a_relative_home_refuses_the_spawn_and_loom_run() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("repo/.loom/work");
    let hooks_dir = temp.path().join("hooks");
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(&hooks_dir).unwrap();
    for hooks_override in [false, true] {
        let case = format!("LOOM_HOOKS_DIR={hooks_override}");
        let mut env = InstallRootsEnv::isolate(temp.path());
        env.set("HOME", "rel/home");
        if hooks_override {
            env.set("LOOM_HOOKS_DIR", &hooks_dir);
        }

        let Err(spawn) = LaunchHost::from_env(&work_dir, temp.path()) else {
            panic!("{case}: the spawn must be refused");
        };
        assert_root_refusal(&spawn, &case);
        let run = run_host_facts(&work_dir).expect_err("loom run must be refused");
        assert_root_refusal(&run, &case);
    }
}
