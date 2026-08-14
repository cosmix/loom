//! Tests for hooks configuration.

use crate::fs::permissions::hooks::loom_hooks_config;
use serde_json::Value;

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
        ("Write", "worktree-file-guard.sh"),
        ("Edit", "plans-path-guard.sh"),
        ("Write", "plans-path-guard.sh"),
        ("Read", "worktree-file-guard.sh"),
        ("Glob", "worktree-file-guard.sh"),
        ("Grep", "worktree-file-guard.sh"),
    ];
    assert_eq!(pre_tool.len(), 27);
    for (entry, (matcher, script)) in pre_tool.iter().zip(expected_prefix) {
        assert_hook(entry, matcher, script);
    }
    for matcher in ["Bash", "Edit", "Write", "Read", "Task", "Agent"] {
        assert!(contains_hook(pre_tool, matcher, "codex-forward-guard.sh"));
    }
    assert!(contains_hook(pre_tool, "Bash", "loom-control-complete.sh"));
    for matcher in ["Write", "Edit", "Task", "Agent"] {
        assert!(contains_hook(pre_tool, matcher, "stage-terminal-guard.sh"));
    }
    assert_lifecycle_hooks(&hooks);
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
    assert_eq!(prompt.len(), 1);
    assert_hook(&prompt[0], "*", "skill-trigger.sh");
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
