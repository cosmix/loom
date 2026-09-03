use std::fs;

use super::*;
use crate::fs::permissions::state_root::{
    is_parent_glob_token_deny, is_token_read_deny, token_read_denies,
};

/// The `permissions.deny` strings in a settings file.
fn deny_entries(path: &std::path::Path) -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    value["permissions"]["deny"]
        .as_array()
        .map(|deny| {
            deny.iter()
                .filter_map(|entry| entry.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// A settings file written before loom globbed the project directory out of
/// its token denies makes Claude Code refuse every `rg`/`grep` run from the
/// project root, bypass-immune. Drive the real check-then-fix path over all
/// four spellings loom used to write.
#[test]
fn repair_rewrites_stale_token_deny_shapes_in_the_main_settings_file() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let resolved = {
        let work_dir = root.path().join(".loom/work");
        fs::create_dir_all(&work_dir).unwrap();
        work_dir.canonicalize().unwrap()
    };
    let settings_path = claude_dir.join("settings.local.json");
    fs::write(
        &settings_path,
        serde_json::json!({
            "permissions": {
                "deny": [
                    format!("Read(/{}/admin.token)", resolved.display()),
                    format!("Read(/{}/user.token)", resolved.display()),
                    "Read(.loom/work/admin.token)",
                    "Read(.loom/work/user.token)",
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| issue.description.starts_with("Stale token deny shape in"))
        .expect("the prompting token deny shape must be reported as an issue");
    assert!(fix_issue(root.path(), &issue).unwrap());

    let deny = deny_entries(&settings_path);
    assert!(
        !deny
            .iter()
            .any(|e| is_token_read_deny(e) && !is_parent_glob_token_deny(e)),
        "no prompting spelling may survive the fix, got: {deny:?}"
    );
    for expected in token_read_denies(&resolved.to_string_lossy()) {
        assert!(deny.contains(&expected), "{expected} missing from {deny:?}");
    }
}

/// A worktree's own `.claude/settings.json` carries the same rules and gets
/// the scalpel rather than a regeneration: stale entries replaced by the
/// parent-glob ones, every other key left exactly as it was.
#[cfg(unix)]
#[test]
fn repair_rewrites_a_stale_token_deny_shape_in_a_worktree() {
    let root = tempfile::tempdir().unwrap();
    let work_dir = root.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let worktree = root.path().join(".worktrees/build-api");
    fs::create_dir_all(worktree.join(".claude")).unwrap();
    fs::create_dir_all(worktree.join(".loom")).unwrap();
    std::os::unix::fs::symlink(&work_dir, worktree.join(".loom/work")).unwrap();
    let resolved = work_dir.canonicalize().unwrap();

    let settings_path = worktree.join(".claude/settings.json");
    fs::write(
        &settings_path,
        serde_json::json!({
            "permissions": { "deny": [format!("Read(/{}/user.token)", resolved.display())] },
            "enabledPlugins": { "codex": true }
        })
        .to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| {
            issue.description.starts_with("Stale token deny shape in")
                && issue.description.contains("build-api")
        })
        .expect("the worktree's stale shape must be reported, naming its path");
    assert!(fix_issue(root.path(), &issue).unwrap());

    assert_eq!(
        deny_entries(&settings_path),
        token_read_denies(&resolved.to_string_lossy()).to_vec(),
        "the scalpel must leave exactly the current token denies behind"
    );
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({ "codex": true }),
        "an unrelated key must survive the scalpel"
    );
}
