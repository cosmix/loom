//! Tests for permission constants

use std::collections::BTreeSet;
use std::fs;
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
    assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
    // Handoffs are the only `.work` subtree a file tool may write; everything
    // else goes through the loom CLI.
    assert!(LOOM_PERMISSIONS.contains(&"Edit(.work/handoffs/**)"));
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
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Edit(.work/handoffs/**)"));
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
        // Nor the broad Edit form it would tempt: `Edit(.work/**)` re-exposes
        // `.work/admin.token` and `.work/user.token` (S-1).
        assert!(
            !perms.contains(&"Edit(.work/**)"),
            "{name} must not grant a broad edit over the .work root"
        );
    }
}

/// A hook script has up to FIVE registration sites; missing any one of them
/// is a silent-death bug -- the script never runs even though every other
/// site looks correct. The checklist for the next hook added:
///
///  1. `fs/permissions/constants.rs`: a `HOOK_*` constant (`include_str!`)
///  2. `fs/permissions/constants.rs`: a `LOOM_HOOKS` row naming that constant
///  3. `install.sh`: `install_hooks_remote()`'s `all_hooks` array
///  4. `install.sh`: `install_hooks()`'s `all_hooks` array
///  5. EITHER `fs/permissions/hooks/config.rs`'s `pre_tool_hooks()` (a global
///     PreToolUse hook) OR `hooks/config.rs`'s `HookEvent` enum + `all()` (a
///     per-session hook) -- whichever kind this hook is. A sourced LIBRARY
///     (`_common.sh`, `_read_discipline.sh`, `_read_ledger.sh`) is never
///     invoked directly and needs none of site 5.
///
/// `install_sh_hook_arrays_match_loom_hooks_exactly` pins sites 1-4 below;
/// `loom_hooks_config_only_names_embedded_hooks` pins site 5's global half
/// against sites 1-2; `hooks_tests.rs::test_hook_event_scripts_are_all_embedded`
/// pins site 5's per-session half against sites 1-2.
fn install_sh_hook_arrays() -> Vec<Vec<String>> {
    let mut arrays = Vec::new();
    let mut rest = INSTALL_SH;
    while let Some(start) = rest.find("all_hooks=(") {
        let after_start = &rest[start + "all_hooks=(".len()..];
        let end = after_start
            .find(')')
            .expect("unterminated all_hooks=( ... ) block in install.sh");
        let hooks: Vec<String> = after_start[..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .map(str::to_string)
            })
            .collect();
        arrays.push(hooks);
        rest = &after_start[end + 1..];
    }
    arrays
}

#[test]
fn installer_does_not_delete_legacy_unprefixed_names() {
    assert!(
        !INSTALL_SH.contains("name#loom-"),
        "install.sh must never derive a bare skill or agent name from a Loom-owned name"
    );
    assert!(
        !INSTALL_SH.contains("skills/$old_name") && !INSTALL_SH.contains("agents/$old_name"),
        "bare names such as rust and custom agent names are user-owned"
    );
}

#[test]
fn local_skill_install_preserves_bare_rust_and_custom_skill() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join(".claude");
    let source_dir = temp.path().join("source-skills");
    fs::create_dir_all(claude_dir.join("skills/rust")).unwrap();
    fs::create_dir_all(claude_dir.join("skills/my-custom-skill")).unwrap();
    fs::create_dir_all(source_dir.join("loom-rust")).unwrap();
    fs::write(claude_dir.join("skills/rust/SKILL.md"), "user rust").unwrap();
    fs::write(claude_dir.join("skills/my-custom-skill/SKILL.md"), "custom").unwrap();
    fs::write(source_dir.join("loom-rust/SKILL.md"), "loom rust").unwrap();
    let manifest = source_dir.join("core-skills.txt");
    fs::write(&manifest, "loom-rust\n").unwrap();

    let output = Command::new("bash")
        .arg("-c")
        .arg(
            "source \"$INSTALL_SH_PATH\"; \
             CLAUDE_DIR=\"$TEST_CLAUDE_DIR\"; SKILLS_MODE=all; \
             install_skills_from_source \"$TEST_SOURCE_DIR\" \"$TEST_MANIFEST\" false",
        )
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .env(
            "INSTALL_SH_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../install.sh"),
        )
        .env("TEST_CLAUDE_DIR", &claude_dir)
        .env("TEST_SOURCE_DIR", &source_dir)
        .env("TEST_MANIFEST", &manifest)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "install function failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(claude_dir.join("skills/rust/SKILL.md")).unwrap(),
        "user rust"
    );
    assert_eq!(
        fs::read_to_string(claude_dir.join("skills/my-custom-skill/SKILL.md")).unwrap(),
        "custom"
    );
    assert_eq!(
        fs::read_to_string(claude_dir.join("skills/loom-rust/SKILL.md")).unwrap(),
        "loom rust"
    );
}

#[test]
fn local_agent_install_preserves_bare_and_custom_agents() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join(".claude");
    fs::create_dir_all(claude_dir.join("agents")).unwrap();
    fs::write(
        claude_dir.join("agents/software-engineer.md"),
        "user-owned bare agent",
    )
    .unwrap();
    fs::write(
        claude_dir.join("agents/my-custom-agent.md"),
        "user-owned custom agent",
    )
    .unwrap();

    let output = Command::new("bash")
        .arg("-c")
        .arg(
            "source \"$INSTALL_SH_PATH\"; \
             CLAUDE_DIR=\"$TEST_CLAUDE_DIR\"; install_agents",
        )
        .env("LOOM_INSTALL_LIB_ONLY", "1")
        .env(
            "INSTALL_SH_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../install.sh"),
        )
        .env("TEST_CLAUDE_DIR", &claude_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "agent install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(claude_dir.join("agents/software-engineer.md")).unwrap(),
        "user-owned bare agent"
    );
    assert_eq!(
        fs::read_to_string(claude_dir.join("agents/my-custom-agent.md")).unwrap(),
        "user-owned custom agent"
    );
    assert!(claude_dir
        .join("agents/loom-software-engineer.md")
        .is_file());
}

#[test]
fn install_sh_hook_arrays_match_loom_hooks_exactly() {
    let arrays = install_sh_hook_arrays();
    assert_eq!(
        arrays.len(),
        2,
        "expected exactly 2 `all_hooks=( ... )` blocks in install.sh \
         (install_hooks_remote and install_hooks) - found {}",
        arrays.len()
    );

    let loom_hooks: BTreeSet<&str> = LOOM_HOOKS.iter().map(|(name, _)| *name).collect();

    for (i, hooks) in arrays.iter().enumerate() {
        let hook_set: BTreeSet<&str> = hooks.iter().map(String::as_str).collect();
        let missing: Vec<_> = loom_hooks.difference(&hook_set).collect();
        let extra: Vec<_> = hook_set.difference(&loom_hooks).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "install.sh all_hooks block #{} does not match LOOM_HOOKS - missing \
             from install.sh: {missing:?}, present in install.sh but not in \
             LOOM_HOOKS: {extra:?}. Fix install.sh's all_hooks array (both \
             install_hooks_remote() and install_hooks()) to match \
             fs/permissions/constants.rs's LOOM_HOOKS.",
            i + 1
        );
    }

    let set0: BTreeSet<&str> = arrays[0].iter().map(String::as_str).collect();
    let set1: BTreeSet<&str> = arrays[1].iter().map(String::as_str).collect();
    assert_eq!(
        set0, set1,
        "install.sh's two all_hooks arrays differ: install_hooks_remote() has \
         {set0:?} but install_hooks() has {set1:?} - a hook present in one and \
         not the other installs on only one of the two install paths (remote \
         curl-pipe vs local clone). Keep both arrays identical."
    );
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
