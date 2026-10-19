//! Tests for the global hook registrations and loom-hook recognition.

use super::*;

#[test]
fn test_worktree_file_guard_registered_for_every_file_tool() {
    let config = loom_hooks_config();
    let pre_tool_use = config
        .get("PreToolUse")
        .expect("PreToolUse must exist")
        .as_array()
        .expect("PreToolUse must be an array");

    // Collect all (matcher, command) pairs from PreToolUse
    let entries: Vec<(String, String)> = pre_tool_use
        .iter()
        .filter_map(|entry| {
            let matcher = entry.get("matcher")?.as_str()?.to_string();
            let command = entry
                .get("hooks")?
                .as_array()?
                .first()?
                .get("command")?
                .as_str()?
                .to_string();
            Some((matcher, command))
        })
        .collect();

    // One canonical guard owns every file-tool path decision. MultiEdit is
    // a distinct matcher from Edit, so it needs its own registration.
    for tool in ["Read", "Write", "Edit", "MultiEdit", "Glob", "Grep"] {
        let found = entries.iter().any(|(matcher, command)| {
            matcher == tool && command.contains("worktree-file-guard.sh")
        });
        assert!(
            found,
            "worktree-file-guard.sh not registered as PreToolUse hook for matcher '{tool}'"
        );
    }
}

#[test]
fn test_plans_path_guard_registered_for_every_write_tool() {
    let config = loom_hooks_config();
    let pre_tool_use = config
        .get("PreToolUse")
        .expect("PreToolUse must exist")
        .as_array()
        .expect("PreToolUse must be an array");

    for tool in ["Write", "Edit", "MultiEdit"] {
        let found = pre_tool_use.iter().any(|entry| {
            let matcher = entry.get("matcher").and_then(|m| m.as_str());
            let command = entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .and_then(|arr| arr.first())
                .and_then(|h| h.get("command"))
                .and_then(|c| c.as_str());
            matcher == Some(tool) && command.is_some_and(|c| c.contains("plans-path-guard.sh"))
        });
        assert!(
            found,
            "plans-path-guard.sh not registered as PreToolUse hook for matcher '{tool}'"
        );
    }
}

#[test]
fn test_completion_control_hook_registered_before_and_after_bash_only() {
    let config = loom_hooks_config();
    for phase in ["PreToolUse", "PostToolUse"] {
        let entries = config[phase].as_array().unwrap();
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| {
                entry["hooks"][0]["command"]
                    .as_str()
                    .is_some_and(|command| command.contains("loom-control-complete.sh"))
            })
            .collect();
        assert_eq!(matches.len(), 1, "unexpected {phase} registration count");
        assert_eq!(matches[0]["matcher"], "Bash");
    }
}

fn home() -> Option<&'static std::path::Path> {
    Some(std::path::Path::new("/home/op"))
}

#[test]
fn a_command_under_a_relocated_hooks_dir_is_a_loom_hook() {
    let dir = std::path::Path::new("/opt/agents/claude/hooks/loom");
    assert!(is_loom_hook_in(
        "/opt/agents/claude/hooks/loom/commit-guard.sh",
        Some(dir),
        home()
    ));
    assert!(!is_loom_hook_in(
        "/opt/agents/claude/hooks/loom-extra/commit-guard.sh",
        Some(dir),
        home()
    ));
}

#[test]
fn default_spellings_match_and_a_foreign_hooks_loom_suffix_does_not() {
    let dir = Some(std::path::Path::new("/opt/agents/claude/hooks/loom"));
    for default in [
        "~/.claude/hooks/loom/commit-guard.sh",
        "$HOME/.claude/hooks/loom/commit-guard.sh",
        "/home/op/.claude/hooks/loom/commit-guard.sh",
    ] {
        assert!(is_loom_hook_in(default, dir, home()), "{default}");
        assert!(is_loom_hook_in(default, None, home()), "{default}");
    }
    // A user's own tree that merely ends in `hooks/loom` is not loom's.
    assert!(!is_loom_hook_in(
        "/srv/tools/hooks/loom/mine.sh",
        dir,
        home()
    ));
    assert!(!is_loom_hook_in(
        "/srv/tools/hooks/loom/mine.sh",
        None,
        home()
    ));
}

/// A project-level `.claude/hooks/loom/` hook is the user's: matching it
/// anywhere in the command would have `remove_loom_hooks` delete it.
#[test]
fn a_project_level_claude_hooks_loom_script_is_the_users() {
    let dir = Some(std::path::Path::new("/home/op/.claude/hooks/loom"));
    for user_hook in [
        "/home/op/src/app/.claude/hooks/loom/format.sh",
        "./.claude/hooks/loom/format.sh",
        ".claude/hooks/loom/format.sh",
        "bash /srv/repo/.claude/hooks/loom/format.sh",
        "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/loom/format.sh",
    ] {
        assert!(!is_loom_hook_in(user_hook, dir, home()), "{user_hook}");
        assert!(!is_loom_hook_in(user_hook, None, None), "{user_hook}");
    }
}

#[test]
#[serial_test::serial]
fn remove_loom_hooks_keeps_a_project_level_hook() {
    let home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(home.path());
    let entry = |command: &str| serde_json::json!({"matcher": "*", "hooks": [{"type": "command", "command": command}]});
    let user = entry("/srv/repo/.claude/hooks/loom/format.sh");
    let mut hooks = vec![entry("~/.claude/hooks/loom/commit-guard.sh"), user.clone()];

    remove_loom_hooks(&mut hooks);
    assert_eq!(hooks, vec![user]);
}
