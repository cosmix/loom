//! Tests for permission constants

use std::collections::BTreeSet;

use serde_json::Value;

use crate::fs::permissions::constants::{LOOM_HOOKS, LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
use crate::fs::permissions::hooks::loom_hooks_config;

#[test]
fn test_loom_permissions_constant() {
    // Main repo permissions - tightened to minimum necessary
    assert!(LOOM_PERMISSIONS.contains(&"Bash(loom *)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(.work/**)"));
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
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(.work/**)"));
    // Only CLAUDE.md files, not all of .claude/
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/CLAUDE.md)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/CLAUDE.md)"));
    // Loom hooks only
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/hooks/loom/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom *)"));
    // Codex forwarding wrapper (guard pins forwarders to one exact invocation)
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(~/.claude/hooks/loom/codex-forward.sh:*)"));
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
    const INSTALL_SH: &str = include_str!("../../../../../install.sh");
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
