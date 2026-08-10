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
