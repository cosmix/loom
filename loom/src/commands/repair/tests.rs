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

/// `settings_local_has_hooks` used to only check for the presence of *a*
/// `hooks` key, so a file that lost every registration but one (e.g. a
/// pre-registration-count settings file) passed the old check and `--fix`
/// never ran. Drive the real check-then-fix path against genuine drift.
#[test]
fn repair_fixes_partially_registered_hooks() {
    let root = tempfile::tempdir().unwrap();
    crate::fs::permissions::ensure_loom_hooks_local(root.path()).unwrap();

    let settings_path = root.path().join(".claude/settings.local.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    let stop_only = settings["hooks"]["Stop"].clone();
    settings["hooks"] = serde_json::json!({ "Stop": stop_only });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| issue.description.contains("hook registration(s) missing"))
        .expect("a settings file missing most hook registrations must be reported as an issue");

    assert!(
        fix_issue(root.path(), &issue).unwrap(),
        "the drifted-hooks issue must be claimed by a fix branch, not silently skipped"
    );
    assert!(crate::fs::permissions::settings_local_hook_drift(root.path()).is_empty());

    // And the repo is then clean on a re-check.
    assert!(!check_all_issues(root.path())
        .iter()
        .any(|issue| issue.description.contains("hook registration(s) missing")));
}

/// A registration pointing at a script loom no longer ships (e.g. left behind
/// by an older loom version) is real drift that presence-only checking could
/// never see, since the file still has a `hooks` key.
#[test]
fn repair_fixes_an_obsolete_hook_registration() {
    let root = tempfile::tempdir().unwrap();
    crate::fs::permissions::ensure_loom_hooks_local(root.path()).unwrap();

    let settings_path = root.path().join(".claude/settings.local.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    let hooks_dir = dirs::home_dir().unwrap().join(".claude/hooks/loom");
    let ghost_command = hooks_dir.join("obsolete-ghost.sh").display().to_string();
    settings["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "matcher": "*",
            "hooks": [{"type": "command", "command": ghost_command}],
        }));
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| issue.description.contains("obsolete hook registration(s)"))
        .expect("a registration for a script loom no longer ships must be reported as an issue");

    assert!(
        fix_issue(root.path(), &issue).unwrap(),
        "the obsolete-registration issue must be claimed by a fix branch, not silently skipped"
    );
    assert!(crate::fs::permissions::settings_local_hook_drift(root.path()).is_empty());

    // And the repo is then clean on a re-check.
    assert!(!check_all_issues(root.path())
        .iter()
        .any(|issue| issue.description.contains("obsolete hook registration(s)")));
}

/// A settings file that predates `worktree.bgIsolation` is otherwise
/// complete, so without a dedicated check `--fix` walks right past it and
/// main-repo subagents keep spawning stray nested worktrees.
#[test]
fn repair_fixes_missing_worktree_bg_isolation() {
    let root = tempfile::tempdir().unwrap();
    crate::fs::permissions::ensure_loom_hooks_local(root.path()).unwrap();

    let settings_path = root.path().join(".claude/settings.local.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    settings.as_object_mut().unwrap().remove("worktree");
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    assert!(!crate::fs::permissions::settings_local_has_worktree_isolation_disabled(root.path()));

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| issue.description.contains("worktree.bgIsolation"))
        .expect("a missing worktree.bgIsolation setting must be reported as an issue");

    assert!(
        fix_issue(root.path(), &issue).unwrap(),
        "the worktree-isolation issue must be claimed by a fix branch, not silently skipped"
    );
    assert!(crate::fs::permissions::settings_local_has_worktree_isolation_disabled(root.path()));
}

/// Stale per-session identity env left in EITHER main-repo settings file
/// shadows the wrapper script's fresh exports in every session of this repo.
/// This is also the test that proves the dispatcher-ordering fix in
/// `fix_issue`: without a dedicated arm ahead of the generic
/// ".claude/settings.local.json" arm, the settings.json copy is never
/// healed (the generic arm's `fix_hooks_local` never touches settings.json).
#[test]
fn repair_scrubs_stale_session_identity_env() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let write = |name: &str, content: serde_json::Value| {
        fs::write(claude_dir.join(name), content.to_string()).unwrap();
    };
    write(
        "settings.json",
        serde_json::json!({ "env": { "LOOM_STAGE_ID": "knowledge-bootstrap" } }),
    );
    write(
        "settings.local.json",
        serde_json::json!({
            "hooks": {},
            "env": {
                "LOOM_WORK_DIR": "/nonexistent/.work",
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
            }
        }),
    );

    let issues = check_all_issues(root.path());
    let find_and_fix = |suffix: &str| {
        let issue = issues
            .iter()
            .find(|issue| {
                issue.description.contains("Stale loom session env in")
                    && issue.description.ends_with(suffix)
            })
            .unwrap_or_else(|| panic!("{suffix}'s stale identity env must be reported"));
        assert!(
            fix_issue(root.path(), issue).unwrap(),
            "the {suffix} identity issue must be claimed by a fix branch"
        );
    };
    find_and_fix("settings.json");
    find_and_fix("settings.local.json");

    assert!(crate::fs::permissions::main_repo_settings_identity_drift(root.path()).is_empty());
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
