use super::*;

#[test]
fn hook_repair_propagates_skill_index_write_failure() {
    let root = tempfile::tempdir().unwrap();
    let error = fix_hooks_with(
        root.path(),
        || Ok(()),
        |_| Ok(()),
        || anyhow::bail!("simulated skill-index write failure"),
    )
    .expect_err("a failed skill-index rebuild must fail the repair action");

    assert!(error
        .to_string()
        .contains("simulated skill-index write failure"));
}

/// A repo whose `.claude/settings.local.json` predates the codex sandbox
/// allowances is otherwise healthy, so `--fix` used to walk right past it and
/// leave every codex run blocked. Drive the real check-then-fix path.
#[test]
fn repair_fixes_a_settings_file_missing_the_codex_allowances() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::json!({
            "hooks": { "PreToolUse": [] },
            "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
            "sandbox": { "filesystem": { "denyRead": ["~/.ssh/**"] } }
        })
        .to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| {
            issue
                .description
                .contains("Codex sandbox allowances missing")
        })
        .expect("a stale settings file must be reported as an issue");

    assert!(
        fix_issue(root.path(), &issue).unwrap(),
        "the codex issue must be claimed by a fix branch, not silently skipped"
    );
    assert!(crate::fs::permissions::settings_local_has_codex_sandbox(
        root.path()
    ));

    // And the repo is then clean on a re-check.
    assert!(!check_all_issues(root.path()).iter().any(|issue| issue
        .description
        .contains("Codex sandbox allowances missing")));
}

/// A settings file written before the knowledge-directory sandbox grant
/// existed can carry `permissions.deny` entries for it in either the
/// enforced `Edit(...)` form or the inert-but-OS-leaking `Write(...)` form.
/// Drive the real check-then-fix path and confirm regeneration leaves
/// neither form behind.
#[test]
fn repair_fixes_a_stale_knowledge_directory_deny() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::json!({
            "hooks": { "PreToolUse": [] },
            "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
            "permissions": {
                "deny": [
                    "Edit(doc/loom/knowledge/**)",
                    "Write(doc/loom/knowledge/**)"
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| issue.description.contains("Stale knowledge-directory deny"))
        .expect("the stale knowledge-directory deny must be reported as an issue");

    assert!(
        fix_issue(root.path(), &issue).unwrap(),
        "the issue must be claimed by a fix branch, not silently skipped"
    );

    let regenerated: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.local.json")).unwrap())
            .unwrap();
    let deny: Vec<&str> = regenerated["permissions"]["deny"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !deny.iter().any(|p| p.contains("doc/loom/knowledge")),
        "neither deny form may survive regeneration, got: {deny:?}"
    );

    // And the repo is then clean on a re-check.
    assert!(!check_all_issues(root.path())
        .iter()
        .any(|issue| issue.description.contains("Stale knowledge-directory deny")));
}

/// The same stale deny inside a worktree's own settings file must also be
/// detected and fixed — each worktree carries its own
/// `.claude/settings.local.json`, independent of the main repo's. Unlike the
/// main repo, the worktree fix must be a SCALPEL, not a regeneration: a
/// worktree's settings file is the sandbox of a possibly live stage session
/// and can legitimately differ from the default (a codex-licensed stage's
/// `~/.codex` grant, a plan's own `allow_write` entry). This seeds the file
/// with both stale deny forms PLUS an unrelated `permissions.allow` entry
/// and a distinctive `sandbox.filesystem.allowWrite` path that
/// `SandboxConfig::default()` would never produce, and asserts both survive
/// the fix — proving the fix strips only the offending deny entries rather
/// than regenerating the file from scratch.
#[test]
fn repair_fixes_a_stale_knowledge_directory_deny_in_a_worktree() {
    let root = tempfile::tempdir().unwrap();
    let worktree_claude_dir = root.path().join(".worktrees/build-api/.claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::json!({
            "permissions": {
                "allow": ["Edit(~/.codex/**)"],
                "deny": [
                    "Edit(doc/loom/knowledge/**)",
                    "Write(doc/loom/knowledge/**)"
                ]
            },
            "sandbox": {
                "filesystem": {
                    "allowWrite": ["~/.codex"]
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| {
            issue.description.contains("Stale knowledge-directory deny")
                && issue.description.contains("build-api")
        })
        .expect("the worktree's stale deny must be reported as an issue, naming its path");

    assert!(fix_issue(root.path(), &issue).unwrap());

    let fixed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(worktree_claude_dir.join("settings.local.json")).unwrap(),
    )
    .unwrap();
    let deny: Vec<&str> = fixed["permissions"]["deny"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !deny.iter().any(|p| p.contains("doc/loom/knowledge")),
        "neither stale deny form may survive the fix, got: {deny:?}"
    );

    // The unrelated codex allowances must survive untouched — a scalpel, not
    // a regeneration from `SandboxConfig::default()` (which carries neither).
    assert_eq!(
        fixed["permissions"]["allow"],
        serde_json::json!(["Edit(~/.codex/**)"]),
        "an unrelated permissions.allow entry must survive the fix, got: {:?}",
        fixed["permissions"]["allow"]
    );
    assert_eq!(
        fixed["sandbox"]["filesystem"]["allowWrite"],
        serde_json::json!(["~/.codex"]),
        "the stage's own sandbox.filesystem.allowWrite must survive the fix, got: {:?}",
        fixed["sandbox"]["filesystem"]["allowWrite"]
    );
}

#[test]
fn loom_run_cmdline_matches_plain_and_pathed() {
    assert!(is_loom_run_cmdline("loom run"));
    assert!(is_loom_run_cmdline("loom run --watch --max-parallel 4"));
    assert!(is_loom_run_cmdline("/usr/local/bin/loom run"));
    assert!(is_loom_run_cmdline(
        "/home/u/.cargo/bin/loom run --no-merge"
    ));
}

#[test]
fn loom_run_cmdline_rejects_non_run_and_unrelated() {
    assert!(!is_loom_run_cmdline("loom status"));
    assert!(!is_loom_run_cmdline("loom stop"));
    assert!(!is_loom_run_cmdline("loom"));
    assert!(!is_loom_run_cmdline("vim loom/src/commands/run/mod.rs"));
    assert!(!is_loom_run_cmdline("cargo run -- loom"));
    assert!(!is_loom_run_cmdline("loomx run"));
    assert!(!is_loom_run_cmdline(""));
}
