use std::fs;

use super::*;

#[test]
fn workspace_repair_returns_nothing_for_a_clean_workspace() {
    let root = tempfile::tempdir().unwrap();

    let applied = repair_workspace_from(root.path(), Vec::new()).unwrap();

    assert!(applied.is_empty());
}

#[test]
fn workspace_repair_adds_the_missing_gitignore_entries() {
    let root = tempfile::tempdir().unwrap();
    let issues = vec![
        RepairIssue {
            severity: Severity::Warning,
            description: ".loom/work not found in .gitignore".to_string(),
            fix_description: "Add .loom/work/ and .loom/work to .gitignore".to_string(),
        },
        RepairIssue {
            severity: Severity::Warning,
            description: ".worktrees not found in .gitignore".to_string(),
            fix_description: "Add .worktrees/ and .worktrees to .gitignore".to_string(),
        },
    ];

    let applied = repair_workspace_from(root.path(), issues).unwrap();

    assert_eq!(applied.len(), 2);
    assert!(applied.iter().any(|a| a.description.contains(".loom/work")));
    assert!(applied.iter().any(|a| a.description.contains(".worktrees")));

    let content = fs::read_to_string(root.path().join(".gitignore")).unwrap();
    assert!(content.lines().any(|l| l.trim() == ".loom/work/"));
    assert!(content.lines().any(|l| l.trim() == ".loom/work"));
    assert!(content.lines().any(|l| l.trim() == ".worktrees/"));
    assert!(content.lines().any(|l| l.trim() == ".worktrees"));
}

#[test]
#[cfg(unix)]
fn workspace_repair_removes_a_corrupted_work_symlink() {
    let root = tempfile::tempdir().unwrap();
    let sibling = root.path().join("sibling-target");
    fs::create_dir_all(&sibling).unwrap();
    fs::create_dir_all(root.path().join(".loom")).unwrap();
    std::os::unix::fs::symlink(&sibling, root.path().join(".loom/work")).unwrap();

    assert!(
        crate::fs::work_integrity::validate_work_dir_state(root.path()).is_err(),
        "a main-repo .loom/work symlink must fail validation before the repair"
    );

    let issue = RepairIssue {
        severity: Severity::Critical,
        description: format!(
            ".loom/work is a symlink (-> {}) in main repo",
            sibling.display()
        ),
        fix_description: "Remove the .loom/work symlink and reinitialize".to_string(),
    };

    let applied = repair_workspace_from(root.path(), vec![issue]).unwrap();

    assert_eq!(applied.len(), 1);
    assert!(!root.path().join(".loom/work").exists());
    assert!(!root.path().join(".loom/work").is_symlink());
    assert!(
        crate::fs::work_integrity::validate_work_dir_state(root.path()).is_ok(),
        "the workspace must pass validation once the symlink is healed"
    );
}

#[test]
fn workspace_repair_leaves_the_invalid_work_shape_to_an_explicit_repair() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".loom")).unwrap();
    fs::write(root.path().join(".loom/work"), "not a directory").unwrap();

    let issue = RepairIssue {
        severity: Severity::Critical,
        description: ".loom/work exists but is neither directory nor symlink".to_string(),
        fix_description: "Remove .loom/work and reinitialize".to_string(),
    };

    let applied = repair_workspace_from(root.path(), vec![issue]).unwrap();

    assert!(
        applied.is_empty(),
        "an invalid work shape must be left to an explicit `loom repair --fix`"
    );
    assert!(root.path().join(".loom/work").exists());
}

#[test]
fn workspace_repair_leaves_home_and_state_fixes_to_an_explicit_repair() {
    let root = tempfile::tempdir().unwrap();
    let issues = vec![
        RepairIssue {
            severity: Severity::Warning,
            description: "Old unprefixed skill 'rust' found (superseded by 'loom-rust')"
                .to_string(),
            fix_description: "Remove ~/.claude/skills/rust (loom-rust already installed)"
                .to_string(),
        },
        RepairIssue {
            severity: Severity::Critical,
            description: "Phantom merge: some-stage marked merged but commit not in main"
                .to_string(),
            fix_description:
                "Revert merged flag to false (manual investigation needed for lost work)"
                    .to_string(),
        },
        RepairIssue {
            severity: Severity::Info,
            description: "Settings.json references old-style skill names".to_string(),
            fix_description: "Update skill references from 'name' to 'loom-name' in settings"
                .to_string(),
        },
    ];

    let applied = repair_workspace_from(root.path(), issues).unwrap();

    assert!(
        applied.is_empty(),
        "settings/state fixes must stay with an explicit `loom repair --fix`, got: {applied:?}"
    );
}

#[test]
fn workspace_repair_classification_matches_the_repair_dispatch() {
    let allow_listed = [
        ".loom/work is a symlink (-> /tmp/x) in main repo",
        ".loom/work not found in .gitignore",
        ".work not found in .gitignore",
        ".worktrees not found in .gitignore",
        "Git pre-commit hook not installed",
        "Project .claude/settings.json incomplete (file missing)",
        "Hooks found in .claude/settings.json (should be in settings.local.json)",
        "Loom hook scripts missing or outdated in ~/.claude/hooks/loom (1 of 5)",
    ];
    for description in allow_listed {
        assert!(
            WorkspaceFix::classify(description).is_some(),
            "{description} must classify as a workspace fix"
        );
    }

    assert!(WorkspaceFix::classify(
        "Old unprefixed skill 'rust' found (superseded by 'loom-rust')"
    )
    .is_none());
}

#[test]
#[cfg(unix)]
fn workspace_check_does_not_flag_a_worktrees_symlink_as_corruption() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree_root = tmp.path().join(".worktrees").join("some-stage");
    fs::create_dir_all(&worktree_root).unwrap();
    let sibling = tmp.path().join("sibling-target");
    fs::create_dir_all(&sibling).unwrap();
    fs::create_dir_all(worktree_root.join(".loom")).unwrap();
    std::os::unix::fs::symlink(&sibling, worktree_root.join(".loom/work")).unwrap();

    let issues = check(&worktree_root);
    assert!(
        !issues
            .iter()
            .any(|i| i.description.contains("is a symlink (->")),
        "a worktree symlink is the correct shape and must not be flagged: {issues:?}"
    );

    // The same symlink shape, at a root NOT under `.worktrees/`, is still
    // corruption - proves the guard discriminates rather than disabling the
    // check outright.
    let main_repo_root = tmp.path().join("main-repo");
    fs::create_dir_all(main_repo_root.join(".loom")).unwrap();
    std::os::unix::fs::symlink(&sibling, main_repo_root.join(".loom/work")).unwrap();

    let issues = check(&main_repo_root);
    assert!(
        issues
            .iter()
            .any(|i| i.description.contains("is a symlink (->")),
        "a main-repo symlink must still be flagged: {issues:?}"
    );
}

#[test]
#[cfg(unix)]
fn workspace_repair_leaves_a_worktree_symlink_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let worktree_root = tmp.path().join(".worktrees").join("some-stage");
    fs::create_dir_all(&worktree_root).unwrap();
    let sibling = tmp.path().join("sibling-target");
    fs::create_dir_all(&sibling).unwrap();
    fs::create_dir_all(worktree_root.join(".loom")).unwrap();
    std::os::unix::fs::symlink(&sibling, worktree_root.join(".loom/work")).unwrap();

    let applied = repair_workspace(&worktree_root).unwrap();

    assert!(
        !applied
            .iter()
            .any(|a| a.description.contains("is a symlink (->")),
        "a worktree symlink must not be repaired away: {applied:?}"
    );
    assert!(
        worktree_root.join(".loom/work").is_symlink(),
        "the worktree symlink must survive repair"
    );
}
