//! `loom repair --fix` against drifted or stale main-repo settings files:
//! hook registrations, worktree isolation, and session identity env.

use std::fs;

use super::*;

/// `settings_local_has_hooks` used to only check for the presence of *a*
/// `hooks` key, so a file that lost every registration but one (e.g. a
/// pre-registration-count settings file) passed the old check and `--fix`
/// never ran. Drive the real check-then-fix path against genuine drift.
#[test]
#[serial_test::serial]
fn repair_fixes_partially_registered_hooks() {
    let isolated_home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(isolated_home.path());
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
#[serial_test::serial]
fn repair_fixes_an_obsolete_hook_registration() {
    let isolated_home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(isolated_home.path());
    let root = tempfile::tempdir().unwrap();
    crate::fs::permissions::ensure_loom_hooks_local(root.path()).unwrap();

    let settings_path = root.path().join(".claude/settings.local.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    let hooks_dir = crate::assets::install::claude_hooks_dir().unwrap();
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
#[serial_test::serial]
fn repair_fixes_missing_worktree_bg_isolation() {
    let isolated_home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(isolated_home.path());
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
#[serial_test::serial]
fn repair_scrubs_stale_session_identity_env() {
    let isolated_home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(isolated_home.path());
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
                "LOOM_WORK_DIR": "/nonexistent/.loom/work",
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
