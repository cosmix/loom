//! Tests for hooks configuration.

use crate::fs::permissions::constants::LOOM_HOOKS;
use crate::fs::permissions::hooks::loom_hooks_config;
use crate::fs::permissions::settings::ensure_loom_permissions_to;
use crate::hooks::HookEvent;
use serde_json::{json, Value};
use tempfile::TempDir;

#[test]
fn test_hooks_config_structure() {
    let hooks = loom_hooks_config();
    let pre_tool = hooks["PreToolUse"].as_array().unwrap();
    let expected_prefix = [
        ("AskUserQuestion", "ask-user-pre.sh"),
        ("Bash", "prefer-modern-tools.sh"),
        ("Bash", "commit-filter.sh"),
        ("Bash", "subagent-verify-guard.sh"),
        ("Bash", "git-add-guard.sh"),
        ("Bash", "worktree-isolation.sh"),
        ("Edit", "worktree-file-guard.sh"),
        ("MultiEdit", "worktree-file-guard.sh"),
        ("Write", "worktree-file-guard.sh"),
        ("NotebookEdit", "worktree-file-guard.sh"),
        ("Edit", "plans-path-guard.sh"),
        ("MultiEdit", "plans-path-guard.sh"),
        ("Write", "plans-path-guard.sh"),
        ("Read", "worktree-file-guard.sh"),
        ("Glob", "worktree-file-guard.sh"),
        ("Grep", "worktree-file-guard.sh"),
    ];
    assert_eq!(pre_tool.len(), 39);
    for (entry, (matcher, script)) in pre_tool.iter().zip(expected_prefix) {
        assert_hook(entry, matcher, script);
    }
    for matcher in ["Bash", "Edit", "Write", "Read", "Task", "Agent"] {
        assert!(contains_hook(pre_tool, matcher, "codex-forward-guard.sh"));
    }
    assert!(contains_hook(pre_tool, "Task", "spawn-guard.sh"));
    assert!(contains_hook(pre_tool, "Agent", "spawn-guard.sh"));
    assert!(contains_hook(pre_tool, "Read", "read-guard.sh"));
    assert!(contains_hook(pre_tool, "Bash", "poll-guard.sh"));
    // `_read_discipline.sh` and `_read_ledger.sh` are sourced libraries, not
    // PreToolUse hooks.
    for library in ["_read_discipline.sh", "_read_ledger.sh"] {
        assert!(!pre_tool.iter().any(|entry| {
            hook_command(entry)
                .rsplit('/')
                .next()
                .is_some_and(|script| script == library)
        }));
    }
    assert!(contains_hook(pre_tool, "Bash", "loom-control-complete.sh"));
    for matcher in ["Write", "Edit", "Task", "Agent"] {
        assert!(contains_hook(pre_tool, matcher, "stage-terminal-guard.sh"));
    }
    assert_notebook_edit_hooks(pre_tool);
    assert_multi_edit_hooks(pre_tool);
    assert_lifecycle_hooks(&hooks);
}

/// Matchers are exact tool names, so a guard registered only for `Edit` does
/// not see `MultiEdit` - which mutates files identically and carries the same
/// `tool_input.file_path`. Every guard wired to `Edit` must therefore also be
/// wired to `MultiEdit`, or that guard is bypassable by choosing the other
/// tool. This asserts the pairing as an invariant over the whole list rather
/// than a fixed set of names, so a guard added for `Edit` later cannot quietly
/// reopen the gap.
fn assert_multi_edit_hooks(pre_tool: &[Value]) {
    let edit_scripts: Vec<&str> = pre_tool
        .iter()
        .filter(|entry| entry["matcher"] == "Edit")
        .map(hook_command)
        .collect();
    assert!(
        !edit_scripts.is_empty(),
        "no Edit-matched hooks found - the pairing invariant would be vacuous"
    );
    for command in edit_scripts {
        let script = command.rsplit('/').next().unwrap();
        assert!(
            contains_hook(pre_tool, "MultiEdit", script),
            "{script} is registered for Edit but not for MultiEdit - MultiEdit bypasses it"
        );
    }
}

fn assert_notebook_edit_hooks(pre_tool: &[Value]) {
    // NotebookEdit is registered against all three guards that gate file
    // mutation / agent-spawn tools; `worktree-file-guard.sh`'s NotebookEdit
    // entry is already checked positionally above, so this focuses on the
    // three guards together for direct coverage of the new registrations.
    for script in [
        "worktree-file-guard.sh",
        "codex-forward-guard.sh",
        "stage-terminal-guard.sh",
    ] {
        assert!(contains_hook(pre_tool, "NotebookEdit", script));
    }
    // `plans-path-guard.sh` and `no-preexisting-failures.sh` are NOT
    // registered for NotebookEdit: both extract `.tool_input.file_path` /
    // `.tool_input.content` and only act on Write/Edit tool calls, so wiring
    // them to NotebookEdit would be a no-op that never fires.
    assert!(!contains_hook(
        pre_tool,
        "NotebookEdit",
        "plans-path-guard.sh"
    ));
    assert!(!contains_hook(
        pre_tool,
        "NotebookEdit",
        "no-preexisting-failures.sh"
    ));
}

/// Pins the session-specific `HookEvent` surface (`loom/src/hooks/config.rs`).
/// A variant added to the enum without bumping this count is a silent
/// regression - this must be bumped deliberately, in the same change that
/// adds/removes an event, alongside `Display`, `script_name()` and `all()`.
#[test]
fn test_hook_event_surface_has_seven_events() {
    assert_eq!(
        HookEvent::all().len(),
        7,
        "HookEvent::all() surface changed - verify Display, script_name(), \
         and to_settings_hooks() were all updated to match"
    );
}

/// Regression test for the "updated 3 of the 4 sites" failure mode: every
/// `HookEvent`'s `script_name()` must be embedded in `LOOM_HOOKS`
/// (`fs/permissions/constants.rs`), or `install_loom_hooks_to` never writes
/// the script to `~/.claude/hooks/loom/` even though a settings.json entry
/// (from `HooksConfig::to_settings_hooks`) points at it.
#[test]
fn test_hook_event_scripts_are_all_embedded() {
    let embedded: Vec<&str> = LOOM_HOOKS.iter().map(|(name, _)| *name).collect();
    for event in HookEvent::all() {
        assert!(
            embedded.contains(&event.script_name()),
            "HookEvent::{event} names script '{}' which is missing from LOOM_HOOKS \
             in fs/permissions/constants.rs - it would never be installed",
            event.script_name()
        );
    }
}

fn assert_lifecycle_hooks(hooks: &Value) {
    let post_tool = hooks["PostToolUse"].as_array().unwrap();
    assert_eq!(post_tool.len(), 2);
    assert_hook(&post_tool[0], "AskUserQuestion", "ask-user-post.sh");
    assert_hook(&post_tool[1], "Bash", "loom-control-complete.sh");

    let stop = hooks["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert_hook(&stop[0], "*", "commit-guard.sh");

    let prompt = hooks["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(prompt.len(), 2);
    assert_hook(&prompt[0], "*", "skill-trigger.sh");
    assert_hook(&prompt[1], "*", "user-prompt-context.sh");
}

fn contains_hook(entries: &[Value], matcher: &str, script: &str) -> bool {
    entries
        .iter()
        .any(|entry| entry["matcher"] == matcher && hook_command(entry).contains(script))
}

fn assert_hook(entry: &Value, matcher: &str, script: &str) {
    assert_eq!(entry["matcher"], matcher);
    assert!(hook_command(entry).contains(script));
}

fn hook_command(entry: &Value) -> &str {
    entry["hooks"][0]["command"].as_str().unwrap()
}

const FOREIGN_COMMAND: &str = "/home/user/.claude/hooks/my-custom-hook.sh";
const STALE_LOOM_COMMAND: &str = "/home/user/.claude/hooks/loom/skill-trigger.sh";

/// Shared fixture for the two tests below: seed `.claude/settings.local.json`
/// with two duplicate pre-existing loom `UserPromptSubmit` hooks and one
/// FOREIGN (non-loom-path) hook, run the loom hooks merge (`hooks.rs`:
/// remove-all-loom-then-reappend, keyed on script basename), and return the
/// resulting `UserPromptSubmit` hook commands in order.
///
/// A fail-safe merge tested only in its happy direction (loom hooks get
/// added) proves nothing about whether a user's own hook survives it, so
/// this fixture backs one test per direction instead of a single test that
/// only reports which assertion tripped without saying which guarantee broke.
fn merge_polluted_user_prompt_submit_hooks() -> Vec<String> {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");
    let claude_dir = repo_root.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    let existing = json!({
        "hooks": {
            "UserPromptSubmit": [
                {"matcher": "*", "hooks": [{"type": "command", "command": STALE_LOOM_COMMAND}]},
                {"matcher": "*", "hooks": [{"type": "command", "command": STALE_LOOM_COMMAND}]},
                {"matcher": "*", "hooks": [{"type": "command", "command": FOREIGN_COMMAND}]},
            ]
        }
    });
    std::fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let content = std::fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    settings["hooks"]["UserPromptSubmit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["hooks"][0]["command"].as_str().unwrap().to_string())
        .collect()
}

/// Duplicate loom `UserPromptSubmit` entries must collapse to exactly one of
/// each of loom's current hooks — not accumulate across `loom init` reruns.
#[test]
fn user_prompt_submit_merge_collapses_duplicates() {
    let commands = merge_polluted_user_prompt_submit_hooks();

    assert_eq!(
        commands
            .iter()
            .filter(|c| c.ends_with("skill-trigger.sh"))
            .count(),
        1,
        "duplicate loom entries were not collapsed: {commands:?}"
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.ends_with("user-prompt-context.sh"))
            .count(),
        1,
        "the new loom hook was not added: {commands:?}"
    );
}

/// A FOREIGN (non-loom-path) `UserPromptSubmit` entry must survive the
/// remove-all-loom-then-reappend merge untouched.
#[test]
fn user_prompt_submit_merge_preserves_foreign_entry() {
    let commands = merge_polluted_user_prompt_submit_hooks();

    assert!(
        commands.iter().any(|c| c == FOREIGN_COMMAND),
        "foreign UserPromptSubmit entry was dropped by the loom merge: {commands:?}"
    );
    assert_eq!(
        commands.len(),
        3,
        "expected 2 loom entries + 1 preserved foreign entry: {commands:?}"
    );
}
