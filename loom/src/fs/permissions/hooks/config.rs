use serde_json::{json, Value};

pub(super) fn build(hooks_dir: &str) -> Value {
    json!({
        "PreToolUse": pre_tool_hooks(hooks_dir),
        "PostToolUse": [
            hook(hooks_dir, "AskUserQuestion", "ask-user-post.sh"),
            hook(hooks_dir, "Bash", "loom-control-complete.sh"),
        ],
        "Stop": [hook(hooks_dir, "*", "commit-guard.sh")],
        "UserPromptSubmit": [hook(hooks_dir, "*", "skill-trigger.sh")],
    })
}

fn pre_tool_hooks(hooks_dir: &str) -> Vec<Value> {
    const HOOKS: &[(&str, &str)] = &[
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
        ("Bash", "no-preexisting-failures.sh"),
        ("Write", "no-preexisting-failures.sh"),
        ("Edit", "no-preexisting-failures.sh"),
        ("Bash", "codex-forward-guard.sh"),
        ("Bash", "loom-control-complete.sh"),
        ("Edit", "codex-forward-guard.sh"),
        ("Write", "codex-forward-guard.sh"),
        ("Read", "codex-forward-guard.sh"),
        ("Task", "codex-forward-guard.sh"),
        ("Agent", "codex-forward-guard.sh"),
        ("Write", "stage-terminal-guard.sh"),
        ("Edit", "stage-terminal-guard.sh"),
        ("Task", "stage-terminal-guard.sh"),
        ("Agent", "stage-terminal-guard.sh"),
    ];
    HOOKS
        .iter()
        .map(|(matcher, script)| hook(hooks_dir, matcher, script))
        .collect()
}

fn hook(hooks_dir: &str, matcher: &str, script: &str) -> Value {
    json!({
        "matcher": matcher,
        "hooks": [{"type": "command", "command": format!("{hooks_dir}/{script}")}],
    })
}
