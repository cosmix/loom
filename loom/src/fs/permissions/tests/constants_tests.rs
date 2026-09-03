//! Tests for permission constants

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

use crate::fs::permissions::constants::{LOOM_HOOKS, LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
use crate::fs::permissions::hooks::loom_hooks_config;

const INSTALL_SH: &str = include_str!("../../../../../install.sh");

#[test]
fn test_loom_permissions_constant() {
    // Main repo permissions - tightened to minimum necessary
    assert!(LOOM_PERMISSIONS.contains(&"Bash(loom *)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(.loom/work/**)"));
    // Handoffs are the only `.loom/work` subtree a file tool may write;
    // everything else goes through the loom CLI.
    assert!(LOOM_PERMISSIONS.contains(&"Edit(.loom/work/handoffs/**)"));
    // Only CLAUDE.md files, not all of .claude/
    assert!(LOOM_PERMISSIONS.contains(&"Read(.claude/CLAUDE.md)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(~/.claude/CLAUDE.md)"));
    // Loom hooks only, not all hooks
    assert!(LOOM_PERMISSIONS.contains(&"Read(~/.claude/hooks/loom/**)"));
    // Codex forwarding wrapper (guard pins forwarders to one exact invocation)
    assert!(LOOM_PERMISSIONS.contains(&"Bash(~/.claude/hooks/loom/codex-forward.sh:*)"));
}

#[test]
fn test_worktree_permissions_constant() {
    // Worktree permissions - same tightened set
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.loom/work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Edit(.loom/work/handoffs/**)"));
    // Only CLAUDE.md files, not all of .claude/
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/CLAUDE.md)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/CLAUDE.md)"));
    // Loom hooks only
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/hooks/loom/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom *)"));
    // Codex forwarding wrapper (guard pins forwarders to one exact invocation)
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(~/.claude/hooks/loom/codex-forward.sh:*)"));
}

#[test]
fn loom_permission_constants_never_grant_a_write_rule() {
    // Claude Code's file permission check consults only `Edit(path)` rules. A
    // `Write(path)` allow grants nothing and prints a warning at every session
    // start — and these constants are copied verbatim into every worktree's
    // settings.json, so one entry here means one warning per stage session.
    for (name, perms) in [
        ("LOOM_PERMISSIONS", LOOM_PERMISSIONS),
        ("LOOM_PERMISSIONS_WORKTREE", LOOM_PERMISSIONS_WORKTREE),
    ] {
        assert!(
            !perms.iter().any(|p| p.starts_with("Write(")),
            "{name} must not contain an inert Write(...) grant, got: {perms:?}"
        );
        // Nor the broad Edit form it would tempt: `Edit(.loom/work/**)`
        // re-exposes `.loom/work/admin.token` and `.loom/work/user.token` (S-1).
        assert!(
            !perms.contains(&"Edit(.loom/work/**)"),
            "{name} must not grant a broad edit over the .loom/work root"
        );
    }
}

/// A hook script has up to THREE registration sites; missing any one of them
/// is a silent-death bug -- the script never runs even though every other
/// site looks correct. The checklist for the next hook added:
///
///  1. `fs/permissions/constants.rs`: a `HOOK_*` constant (`include_str!`)
///  2. `fs/permissions/constants.rs`: a `LOOM_HOOKS` row naming that constant
///  3. EITHER `fs/permissions/hooks/config.rs`'s `pre_tool_hooks()` (a global
///     PreToolUse hook) OR `hooks/config.rs`'s `HookEvent` enum + `all()` (a
///     per-session hook) -- whichever kind this hook is. A sourced LIBRARY
///     (`_common.sh`, `_read_discipline.sh`, `_read_ledger.sh`) is never invoked
///     directly and needs no third site.
///
/// `loom_hooks_config_only_names_embedded_hooks` pins site 3's global half
/// against sites 1-2; `hooks_tests.rs::test_hook_event_scripts_are_all_embedded`
/// pins site 3's per-session half against sites 1-2.
// Per-asset preservation coverage moved to loom/src/assets/tests.rs when
// install.sh began delegating asset placement to the binary.
#[test]
fn install_sh_delegates_asset_placement_to_the_binary() {
    assert_eq!(
        INSTALL_SH
            .matches(r#""$loom_bin" install-assets --skills "$SKILLS_MODE""#)
            .count(),
        1,
        "install.sh must contain exactly one literal asset-placement delegation"
    );
    assert_eq!(
        INSTALL_SH.matches("install-assets --skills").count(),
        1,
        "install.sh must contain install-assets --skills exactly once"
    );
}

#[test]
fn install_sh_carries_no_per_asset_copy_loops() {
    assert!(
        !INSTALL_SH.contains("all_hooks"),
        "all_hooks reappeared in install.sh"
    );
    assert!(
        !INSTALL_SH.contains("agents.zip"),
        "agents.zip reappeared in install.sh"
    );
    assert!(
        !INSTALL_SH.contains("skills.zip"),
        "skills.zip reappeared in install.sh"
    );
    assert!(
        !INSTALL_SH.contains("download_and_extract_zip"),
        "download_and_extract_zip reappeared in install.sh"
    );
    assert!(
        !INSTALL_SH.contains("update_completions"),
        "update_completions reappeared in install.sh"
    );
    assert!(
        !INSTALL_SH.contains("cleanup_backups"),
        "cleanup_backups reappeared in install.sh"
    );
}

#[test]
fn install_sh_still_validates_the_skills_flag() {
    let temp = TempDir::new().unwrap();
    let installer_path = temp.path().join("install.sh");
    let home = temp.path().join("home");
    fs::write(&installer_path, INSTALL_SH).unwrap();
    fs::create_dir_all(&home).unwrap();

    let bogus = Command::new("bash")
        .arg("-c")
        .arg(r#"source "$INSTALL_SH_PATH"; parse_args --skills bogus"#)
        .env("HOME", &home)
        .env("INSTALL_SH_PATH", &installer_path)
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .output()
        .unwrap();
    assert!(
        !bogus.status.success(),
        "parse_args must reject an invalid --skills value; stderr: {}",
        String::from_utf8_lossy(&bogus.stderr)
    );

    let all = Command::new("bash")
        .arg("-c")
        .arg(r#"source "$INSTALL_SH_PATH"; parse_args --skills all; printf '%s' "$SKILLS_MODE""#)
        .env("HOME", &home)
        .env("INSTALL_SH_PATH", &installer_path)
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .output()
        .unwrap();
    assert!(
        all.status.success(),
        "parse_args must accept --skills all; stderr: {}",
        String::from_utf8_lossy(&all.stderr)
    );
    assert_eq!(all.stdout, b"all", "--skills all must set SKILLS_MODE=all");
}

/// Materialize a copy of `install.sh`, a stub `loom` binary that logs its
/// argv, and the `$HOME` both run under. Returns `(temp, installer_path,
/// home, argv_log)`; the caller must keep `temp` alive for the paths to
/// remain valid.
#[cfg(unix)]
fn stage_install_sh_with_stub_binary() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let installer_path = temp.path().join("install.sh");
    let home = temp.path().join("home");
    let loom_bin = home.join(".local/bin/loom");
    let argv_log = temp.path().join("argv");
    fs::write(&installer_path, INSTALL_SH).unwrap();
    fs::create_dir_all(loom_bin.parent().unwrap()).unwrap();
    fs::write(
        &loom_bin,
        r#"#!/usr/bin/env bash
echo "$@" >> "$LOOM_STUB_ARGV_LOG"
"#,
    )
    .unwrap();
    fs::set_permissions(&loom_bin, fs::Permissions::from_mode(0o755)).unwrap();

    (temp, installer_path, home, argv_log)
}

#[test]
fn install_sh_invokes_the_binary_exactly_once_with_the_resolved_mode() {
    let (_temp, installer_path, home, argv_log) = stage_install_sh_with_stub_binary();

    let output = Command::new("bash")
        .arg("-c")
        .arg(
            r#"source "$INSTALL_SH_PATH"; install_loom_local() { :; }; install_loom_remote() { :; }; confirm_overwrites() { return 0; }; main --skills core"#,
        )
        .env("HOME", &home)
        .env("INSTALL_SH_PATH", &installer_path)
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .env("LOOM_STUB_ARGV_LOG", &argv_log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "main --skills core failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let calls = fs::read_to_string(&argv_log)
        .expect("the temporary loom stub must receive installer invocations")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        calls.last().map(String::as_str),
        Some("install-assets --skills core"),
        "the final loom invocation must delegate assets using the resolved skills mode"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.contains("install-assets --skills"))
            .count(),
        1,
        "install.sh must invoke install-assets --skills exactly once"
    );

    for asset_dir in [home.join(".claude"), home.join(".codex")] {
        assert!(
            !asset_dir.exists() || fs::read_dir(&asset_dir).unwrap().next().is_none(),
            "the shell installer must not create assets at {}",
            asset_dir.display()
        );
    }
}

/// Regression test for a typo in `fs/permissions/hooks/config.rs`'s global
/// `pre_tool_hooks()`/`build()` list registering a script name that names no
/// embedded `HOOK_*` constant: nothing else notices, since `loom_hooks_config`
/// just formats whatever basename it is given into a command path.
#[test]
fn loom_hooks_config_only_names_embedded_hooks() {
    let config = loom_hooks_config();
    let loom_hook_names: BTreeSet<&str> = LOOM_HOOKS.iter().map(|(name, _)| *name).collect();

    let Value::Object(events) = &config else {
        panic!("loom_hooks_config() did not return a JSON object");
    };
    for (event, entries) in events {
        for entry in entries.as_array().unwrap_or_else(|| {
            panic!("loom_hooks_config()[{event}] is not an array");
        }) {
            let commands = entry["hooks"].as_array().unwrap_or_else(|| {
                panic!("loom_hooks_config()[{event}] entry has no \"hooks\" array");
            });
            for command_entry in commands {
                let command = command_entry["command"].as_str().unwrap_or_else(|| {
                    panic!("loom_hooks_config()[{event}] hook entry has no \"command\" string");
                });
                let basename = command.rsplit('/').next().unwrap_or(command);
                assert!(
                    loom_hook_names.contains(basename),
                    "loom_hooks_config()[{event}] registers '{command}' but \
                     '{basename}' is not in LOOM_HOOKS (fs/permissions/constants.rs) \
                     - it would never be installed to ~/.claude/hooks/loom/. Add a \
                     HOOK_* const and LOOM_HOOKS row for it, or fix the typo in \
                     fs/permissions/hooks/config.rs."
                );
            }
        }
    }
}
