//! Tests for `Write(...)` rule pruning and deny migration.

use crate::fs::permissions::settings::{ensure_loom_hooks_local, ensure_loom_permissions_to};
use crate::fs::permissions::write_rules::migrate_inert_write_denies;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_migrate_inert_write_denies_rewrites_drops_and_passes_through() {
    let rules: Vec<String> = [
        "Write(~/.bashrc)",            // rewritten to the enforceable form
        "Write(**)",                   // blanket: dropped, never enforced
        "Write(*)",                    // blanket: dropped
        "Write(../../**)",             // parent traversal: dropped
        "Write(doc/loom/knowledge/x)", // knowledge write channel: dropped
        "Edit(.work/stages/**)",       // already enforceable: untouched
        "Write(.work/stages/**)",      // collapses onto the Edit above
        "Read(~/.ssh/**)",             // not a write rule: untouched
        "Bash(rm -rf:*)",              // not a file rule at all: untouched
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert_eq!(
        migrate_inert_write_denies(&rules),
        vec![
            "Edit(~/.bashrc)",
            "Edit(.work/stages/**)",
            "Read(~/.ssh/**)",
            "Bash(rm -rf:*)",
        ]
    );
}

#[test]
fn test_ensure_loom_permissions_prunes_legacy_work_write_grants() {
    // Every spelling of the `.work` write grant loom itself used to emit: the
    // relative one from LOOM_PERMISSIONS, the worktree-relative one that
    // reached this file through `sync`, and the resolved-absolute one from
    // `git/worktree/settings.rs`. All three are inert (Claude Code's file
    // permission check consults only `Edit(path)`) and all three print a
    // warning at every session start until they are removed.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let polluted = json!({
        "permissions": {
            "allow": [
                "Write(.work/**)",
                "Write(../../.work/**)",
                "Write(//home/dev/project/.work/**)",
                "Write(src/**)",
                "Read(src/**)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&polluted).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&temp_dir.path().join("hooks"))).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let allow = settings["permissions"]["allow"].as_array().unwrap();

    let has_legacy_grant = allow.iter().any(|v| {
        v.as_str()
            .is_some_and(|s| s.starts_with("Write(") && s.ends_with(".work/**)"))
    });
    assert!(
        !has_legacy_grant,
        "loom's own inert .work write grants must be pruned, got: {allow:?}"
    );
    // A `Write(...)` entry that is NOT one of loom's own is the developer's
    // config: inert or not, removing it is not loom's call.
    assert!(allow.iter().any(|v| v == "Write(src/**)"));
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    // The replacement grant is in place.
    assert!(allow.iter().any(|v| v == "Edit(.work/handoffs/**)"));
}

#[test]
fn test_ensure_loom_hooks_local_heals_inert_write_denies() {
    // `loom init` / `loom repair` heal an already-polluted local file rather
    // than waiting for the next stage spawn to regenerate it.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let polluted = json!({
        "permissions": {
            "deny": [
                "Write(**)",
                "Write(.work/sessions/**)",
                "Edit(.work/stages/**)",
                "Write(.work/stages/**)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&polluted).unwrap(),
    )
    .unwrap();

    ensure_loom_hooks_local(repo_root).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert_eq!(
        deny_strs,
        vec!["Edit(.work/sessions/**)", "Edit(.work/stages/**)"],
        "blanket deny dropped, the rest migrated and collapsed onto the \
         Edit(...) entry already present"
    );
}
