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
fn test_completion_control_hook_registered_before_bash_only() {
    let config = loom_hooks_config();
    for (phase, expected) in [("PreToolUse", 1), ("PostToolUse", 0)] {
        let entries = config[phase].as_array().unwrap();
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| {
                entry["hooks"][0]["command"]
                    .as_str()
                    .is_some_and(|command| command.contains("loom-control-complete.sh"))
            })
            .collect();
        assert_eq!(
            matches.len(),
            expected,
            "unexpected {phase} registration count"
        );
        if let Some(entry) = matches.first() {
            assert_eq!(entry["matcher"], "Bash");
        }
    }
}

/// A shell-running tool outside the `Bash` matcher must not be able to quote a
/// minted invocation id unchecked.
#[test]
fn the_codex_forward_guard_covers_every_tool_that_runs_a_shell_command() {
    let config = loom_hooks_config();
    let entries = config["PreToolUse"].as_array().unwrap();
    for tool in ["Bash", "Monitor", "PowerShell"] {
        assert!(
            entries.iter().any(|entry| {
                entry["matcher"] == tool
                    && entry["hooks"][0]["command"]
                        .as_str()
                        .is_some_and(|command| command.contains("codex-forward-guard.sh"))
            }),
            "codex-forward-guard.sh not registered for matcher '{tool}'"
        );
    }
}

fn home() -> Option<&'static std::path::Path> {
    Some(std::path::Path::new("/home/op"))
}

/// A `~/.claude` symlinked into a dotfiles tree resolves the hooks directory
/// to its target; a command spelled through that target is still loom's.
#[test]
fn a_command_under_the_resolved_hooks_dir_is_a_loom_hook() {
    let dir = std::path::Path::new("/srv/dotfiles/claude/hooks/loom");
    assert!(is_loom_hook_in(
        "/srv/dotfiles/claude/hooks/loom/commit-guard.sh",
        Some(dir),
        home()
    ));
    assert!(!is_loom_hook_in(
        "/srv/dotfiles/claude/hooks/loom-extra/commit-guard.sh",
        Some(dir),
        home()
    ));
}

#[test]
fn default_spellings_match_and_a_foreign_hooks_loom_suffix_does_not() {
    let dir = Some(std::path::Path::new("/srv/dotfiles/claude/hooks/loom"));
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

/// Loom's only registrations directly under `~/.claude/hooks/` were these
/// three; a user's own script there can share any other shipped hook's name.
#[test]
fn only_the_scripts_loom_once_installed_at_top_level_are_legacy_owned() {
    for legacy in ["commit-guard.sh", "ask-user-pre.sh", "ask-user-post.sh"] {
        for prefix in [
            "~/.claude/hooks",
            "$HOME/.claude/hooks",
            "/home/op/.claude/hooks",
        ] {
            let command = format!("{prefix}/{legacy}");
            assert!(is_loom_hook_in(&command, None, home()), "{command}");
        }
    }
    for user_hook in [
        "session-start.sh",
        "post-tool-use.sh",
        "pre-compact.sh",
        "_common.sh",
    ] {
        for prefix in [
            "~/.claude/hooks",
            "$HOME/.claude/hooks",
            "/home/op/.claude/hooks",
        ] {
            let command = format!("{prefix}/{user_hook}");
            assert!(!is_loom_hook_in(&command, None, home()), "{command}");
        }
    }
}

#[test]
fn reconciliation_keeps_a_users_script_named_like_a_shipped_hook() {
    let user = serde_json::json!({"command": "~/.claude/hooks/session-start.sh"});
    let mut settings = serde_json::json!({"hooks": {"SessionStart": [
        {"matcher": "*", "hooks": [user.clone(), {"command": "~/.claude/hooks/commit-guard.sh"}]}
    ]}});
    let object = settings.as_object_mut().unwrap();

    configure_loom_hooks_with_dir(object, "/installed/loom").unwrap();

    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"],
        serde_json::json!([user])
    );
}

/// A home directory with a space must register a command the shell runs as one
/// word, and that registration must be recognised on the next init.
#[test]
fn a_hooks_dir_with_shell_metacharacters_registers_one_runnable_word_once() {
    for name in ["my user", "o'brien", "a$b", "wow!", "a!b'c!"] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join(name).join(".claude/hooks/loom");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("commit-guard.sh");
        std::fs::write(&script, "#!/bin/sh\necho ran\n").unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let dir_text = dir.display().to_string();

        let mut settings = serde_json::json!({});
        let object = settings.as_object_mut().unwrap();
        configure_loom_hooks_with_dir(object, &dir_text).unwrap();
        let once = object.clone();
        configure_loom_hooks_with_dir(object, &dir_text).unwrap();
        assert_eq!(&once, object, "{name}: a second init appended handlers");

        let stop = settings["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(extract_command_basename(stop), "commit-guard.sh", "{name}");
        assert!(is_loom_hook_in(stop, Some(&dir), None), "{name}");
        let ran = std::process::Command::new("sh")
            .args(["-c", stop])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout),
            "ran\n",
            "{name}: {stop}"
        );

        let drift = super::super::drift::hook_drift_for_dir(&settings, &dir_text);
        assert!(
            drift.missing.is_empty() && drift.obsolete.is_empty(),
            "{name}"
        );
    }
}

#[test]
fn the_unexpanded_default_dir_stays_unquoted_for_the_shell_to_expand() {
    let config = loom_hooks_config_for_dir("~/.claude/hooks/loom");
    assert_eq!(
        config["Stop"][0]["hooks"][0]["command"],
        "~/.claude/hooks/loom/commit-guard.sh"
    );
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

#[test]
fn reconciliation_preserves_user_handlers_and_distinct_registrations() {
    let mut settings = serde_json::json!({"hooks": {"PreToolUse": [
        {"matcher":"Bash", "extra": 42, "hooks":[
            {"command":"/custom/check.sh"}, {"command":"~/.claude/hooks/commit-guard.sh"},
            {"command":"~/.claude/hooks/custom.sh"}]},
        {"matcher":"Write", "hooks":[{"command":"/custom/check.sh"}]},
        {"matcher":"Bash", "hooks":[{"command":"/other/check.sh"}]}
    ]}});
    let object = settings.as_object_mut().unwrap();
    configure_loom_hooks_with_dir(object, "/installed/loom").unwrap();
    let first = serde_json::Value::Object(object.clone());
    configure_loom_hooks_with_dir(object, "/installed/loom").unwrap();
    assert_eq!(first, serde_json::Value::Object(object.clone()));
    let rules = settings["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(rules[0]["extra"], 42);
    assert_eq!(
        rules[0]["hooks"],
        serde_json::json!([
        {"command":"/custom/check.sh"}, {"command":"~/.claude/hooks/custom.sh"}])
    );
    assert_eq!(rules[1]["matcher"], "Write");
    assert_eq!(rules[2]["hooks"][0]["command"], "/other/check.sh");
}
