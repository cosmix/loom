//! Host-fact resolution: the recorded daemon binary, and the refusal an
//! unresolvable install root raises before a spawn or `loom run` goes ahead.

use std::fs;

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::assets::install::test_env::{InstallRootsEnv, CLAUDE_OVERRIDE, CODEX_OVERRIDE};
use crate::assets::install::INSTALL_ROOTS_RECORD;

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

/// `error` is the preflight refusal naming `source` as the bad root.
fn assert_root_refusal(error: &anyhow::Error, source: &str, case: &str) {
    let text = format!("{error:#}");
    assert!(error.is::<SandboxPreflightRefusal>(), "{case}: {text}");
    assert!(
        text.contains("install roots do not resolve")
            && text.contains(source)
            && text.contains("must be an absolute path"),
        "{case}: {text}"
    );
}

/// A valid relocated Claude root beside a relative Codex root, from the
/// environment or the install record, refuses both a spawn and `loom run`,
/// with or without `LOOM_HOOKS_DIR`: neither may go ahead without the
/// relocated-root denies.
#[test]
#[serial]
fn an_unresolvable_install_root_refuses_the_spawn_and_loom_run() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let work_dir = temp.path().join("repo/.loom/work");
    let hooks_dir = temp.path().join("hooks");
    let claude_root = temp.path().join("opt/claude");
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::create_dir_all(&home).unwrap();
    for (recorded, hooks_override) in [(false, false), (false, true), (true, false), (true, true)] {
        let case = format!("recorded={recorded} LOOM_HOOKS_DIR={hooks_override}");
        let mut env = InstallRootsEnv::isolate(&home);
        let record = home.join(INSTALL_ROOTS_RECORD);
        let _ = fs::remove_file(&record);
        let source = if recorded {
            fs::create_dir_all(record.parent().unwrap()).unwrap();
            let entries = format!(
                "claude_dir = \"{}\"\ncodex_dir = \"rel/codex\"\n",
                claude_root.display()
            );
            fs::write(&record, entries).unwrap();
            "the codex_dir entry of"
        } else {
            env.set(CLAUDE_OVERRIDE, &claude_root);
            env.set(CODEX_OVERRIDE, "rel/codex");
            CODEX_OVERRIDE
        };
        if hooks_override {
            env.set("LOOM_HOOKS_DIR", &hooks_dir);
        }

        let Err(spawn) = LaunchHost::from_env(&work_dir, temp.path()) else {
            panic!("{case}: the spawn must be refused");
        };
        assert_root_refusal(&spawn, source, &case);
        let run = run_host_facts(&work_dir).expect_err("loom run must be refused");
        assert_root_refusal(&run, source, &case);
    }
}
