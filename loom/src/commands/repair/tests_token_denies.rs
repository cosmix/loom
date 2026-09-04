use std::fs;

use super::*;
use crate::fs::permissions::state_root::{is_loom_written_read_deny, CREDENTIAL_DENY_READ_PATHS};

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

/// A settings file written before loom stopped writing `Read(...)` denies
/// altogether makes Claude Code refuse every `rg`/`grep` run from the project
/// root, bypass-immune and independent of the rule's path shape. Drive the
/// real check-then-fix path over the daemon-token spellings loom used to
/// write in the main repo's `settings.local.json`, which is regenerated
/// wholesale rather than scalpelled.
#[test]
fn repair_regenerates_the_main_settings_file_with_no_read_deny_left() {
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
        .find(|issue| issue.description.starts_with("Read deny rule in"))
        .expect("the loom-written Read deny rule must be reported as an issue");
    assert!(fix_issue(root.path(), &issue).unwrap());

    let deny = deny_entries(&settings_path);
    assert!(
        !deny.iter().any(|e| e.starts_with("Read(")),
        "no Read( deny rule may survive the fix, got: {deny:?}"
    );
}

/// A worktree's own `.claude/settings.json` carries the same rule and gets
/// the scalpel rather than a regeneration: the loom-written entry is
/// removed, nothing pushed back in its place, and every other key left
/// exactly as it was.
#[cfg(unix)]
#[test]
fn repair_strips_a_loom_written_read_deny_from_a_worktree() {
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
            issue.description.starts_with("Read deny rule in")
                && issue.description.contains("build-api")
        })
        .expect("the worktree's loom-written Read deny rule must be reported, naming its path");
    assert!(fix_issue(root.path(), &issue).unwrap());

    assert!(
        deny_entries(&settings_path).is_empty(),
        "the scalpel must strip the loom-written entry and push nothing back"
    );
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({ "codex": true }),
        "an unrelated key must survive the scalpel"
    );
}

/// A settings file written by an old loom version can carry both the
/// daemon-token denies and the credential-path mirrors it used to write, in
/// the main repo's own `.claude/settings.json` (scalpelled, not
/// regenerated, since only `settings.local.json` is). Detection and the fix
/// must cover every one of those spellings.
#[test]
fn repair_strips_every_loom_written_read_deny_spelling() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let resolved = {
        let work_dir = root.path().join(".loom/work");
        fs::create_dir_all(&work_dir).unwrap();
        work_dir.canonicalize().unwrap()
    };

    let mut deny = vec![
        format!("Read(/{}/admin.token)", resolved.display()),
        format!("Read(/{}/user.token)", resolved.display()),
    ];
    deny.extend(
        CREDENTIAL_DENY_READ_PATHS
            .iter()
            .map(|path| format!("Read({path})")),
    );

    let settings_path = claude_dir.join("settings.json");
    fs::write(
        &settings_path,
        serde_json::json!({ "permissions": { "deny": deny } }).to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| {
            issue.description.starts_with("Read deny rule in")
                && issue.description.contains("settings.json")
        })
        .expect("the loom-written Read deny rules must be reported as one issue");
    assert!(fix_issue(root.path(), &issue).unwrap());

    let deny = deny_entries(&settings_path);
    assert!(
        !deny.iter().any(|e| is_loom_written_read_deny(e)),
        "no loom-written Read deny spelling may survive the fix, got: {deny:?}"
    );
}

/// An operator's own `Read(...)` deny rule must be flagged so the prompting
/// hazard stays visible, but never removed — loom does not know what the
/// rule protects.
#[test]
fn repair_leaves_an_operators_own_read_deny_rule_in_place() {
    let root = tempfile::tempdir().unwrap();
    let claude_dir = root.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::create_dir_all(root.path().join(".loom/work")).unwrap();

    let settings_path = claude_dir.join("settings.json");
    fs::write(
        &settings_path,
        serde_json::json!({ "permissions": { "deny": ["Read(secrets/**)"] } }).to_string(),
    )
    .unwrap();

    let issue = check_all_issues(root.path())
        .into_iter()
        .find(|issue| {
            issue
                .description
                .starts_with("Operator-authored Read deny rule")
                && issue.description.contains("Read(secrets/**)")
        })
        .expect("the operator's own Read deny rule must be reported");
    assert!(
        !fix_issue(root.path(), &issue).unwrap(),
        "loom must not claim to have fixed an operator-authored deny rule"
    );

    assert_eq!(
        deny_entries(&settings_path),
        vec!["Read(secrets/**)".to_string()],
        "the operator's rule must survive untouched"
    );
}
