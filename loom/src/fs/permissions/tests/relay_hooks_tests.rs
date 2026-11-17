//! Registration and installation of the relay guard, the relay Stop hook and
//! their shared kind library.

use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

use crate::fs::permissions::hooks::{install_loom_hooks_to, loom_hooks_config};

fn registered(event: &str, matcher: &str, script: &str) -> usize {
    loom_hooks_config()[event]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["matcher"] == matcher && command_of(entry).ends_with(script))
        .count()
}

fn command_of(entry: &Value) -> &str {
    entry["hooks"][0]["command"].as_str().unwrap()
}

#[test]
fn the_relay_guard_is_a_global_pre_tool_hook_registered_once() {
    assert_eq!(registered("PreToolUse", "Bash", "/loom-relay-guard.sh"), 1);
}

#[test]
fn the_relay_stop_hook_is_a_global_stop_hook_registered_once() {
    assert_eq!(registered("Stop", "*", "/loom-relay-stop.sh"), 1);
}

/// The source-tree hook tests run from the repository and cannot see a helper
/// missing from `LOOM_HOOKS`; this installs into a directory and sources from
/// there, as an installed `loom-relay.sh` does.
#[test]
fn installed_hooks_include_the_relay_kind_library_and_it_sources_alone() {
    let dir = TempDir::new().unwrap();
    let hooks_dir = dir.path().join("hooks/loom");
    install_loom_hooks_to(&hooks_dir).unwrap();
    for name in [
        "_relay_kinds.sh",
        "loom-relay-guard.sh",
        "loom-relay-stop.sh",
    ] {
        assert!(hooks_dir.join(name).is_file(), "{name} was not installed");
    }
    let library = hooks_dir.join("_relay_kinds.sh");
    for (kind, control) in [("commit", true), ("file-dispute", true), ("memory", false)] {
        let status = Command::new("bash")
            .arg("-c")
            .arg(format!(
                "source '{}'; relay_kind_is_control {kind}",
                library.display()
            ))
            .status()
            .unwrap();
        assert_eq!(status.success(), control, "relay_kind_is_control {kind}");
    }
}
