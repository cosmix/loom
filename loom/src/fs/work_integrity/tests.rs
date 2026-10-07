use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_check_work_dir_state_missing() {
    let temp = TempDir::new().unwrap();
    let state = check_work_dir_state(temp.path());
    assert_eq!(state, WorkDirState::Missing);
}

#[test]
fn test_check_work_dir_state_directory() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom").join("work")).unwrap();
    let state = check_work_dir_state(temp.path());
    assert_eq!(state, WorkDirState::Directory);
}

#[test]
#[cfg(unix)]
fn test_check_work_dir_state_symlink() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::create_dir(temp.path().join(".loom")).unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join(".loom").join("work")).unwrap();

    let state = check_work_dir_state(temp.path());
    match state {
        WorkDirState::Symlink { .. } => (),
        other => panic!("Expected Symlink, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn test_check_work_dir_state_broken_symlink_is_symlink_not_missing() {
    // Pins `exists() || is_symlink()` in `state_dir`: a broken symlink
    // must not be mistaken for an absent state directory, since that is
    // exactly the corruption this module exists to catch.
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join(".loom")).unwrap();
    let missing_target = temp.path().join("does-not-exist");
    std::os::unix::fs::symlink(&missing_target, temp.path().join(".loom").join("work")).unwrap();

    let state = check_work_dir_state(temp.path());
    match state {
        WorkDirState::Symlink { .. } => (),
        other => panic!("Expected Symlink, got {:?}", other),
    }
}

#[test]
fn test_check_work_dir_state_legacy_directory() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".work")).unwrap();
    let state = check_work_dir_state(temp.path());
    assert_eq!(state, WorkDirState::Directory);
}

#[test]
fn test_state_dir_defaults_to_nested_when_neither_exists() {
    let temp = TempDir::new().unwrap();
    let (path, layout) = state_dir(temp.path());
    assert_eq!(layout, Layout::Nested);
    assert_eq!(path, temp.path().join(".loom").join("work"));
    assert_eq!(check_work_dir_state(temp.path()), WorkDirState::Missing);
}

#[test]
fn test_state_dir_bare_loom_cache_is_not_a_workspace() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom").join("cache")).unwrap();

    let (path, layout) = state_dir(temp.path());
    assert_eq!(layout, Layout::Nested);
    assert_eq!(path, temp.path().join(".loom").join("work"));
    assert_eq!(check_work_dir_state(temp.path()), WorkDirState::Missing);
}

#[test]
fn test_state_dir_nested_wins_when_both_exist() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom").join("work")).unwrap();
    fs::create_dir_all(temp.path().join(".work")).unwrap();

    let (path, layout) = state_dir(temp.path());
    assert_eq!(layout, Layout::Nested);
    assert_eq!(path, temp.path().join(".loom").join("work"));
}

#[test]
fn test_is_in_worktree() {
    use std::path::PathBuf;

    assert!(is_in_worktree(&PathBuf::from("/foo/.worktrees/my-stage")));
    assert!(is_in_worktree(&PathBuf::from(
        "/foo/.worktrees/my-stage/src"
    )));
    assert!(!is_in_worktree(&PathBuf::from("/foo/bar")));
    assert!(!is_in_worktree(&PathBuf::from("/foo/worktrees/bar")));
}

#[test]
fn test_validate_work_dir_state_main_repo_ok() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom").join("work")).unwrap();
    assert!(validate_work_dir_state(temp.path()).is_ok());
}

#[test]
fn test_validate_work_dir_state_main_repo_missing_ok() {
    let temp = TempDir::new().unwrap();
    // No .loom/work - should be ok (will be created)
    assert!(validate_work_dir_state(temp.path()).is_ok());
}

#[test]
fn test_validate_work_dir_state_legacy_directory_ok() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".work")).unwrap();
    assert!(validate_work_dir_state(temp.path()).is_ok());
}

#[test]
#[cfg(unix)]
fn test_validate_work_dir_state_legacy_symlink_main_repo_err() {
    // Regression pin: before `state_dir` was layout-aware, this returned
    // `Ok` because `check_work_dir_state` only ever inspected
    // `.loom/work`, so the detector reported `Missing` and the CRITICAL
    // bail below never fired for a legacy-layout project.
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join(".work")).unwrap();

    assert!(validate_work_dir_state(temp.path()).is_err());
}

#[test]
#[cfg(unix)]
fn test_validate_work_dir_state_nested_symlink_main_repo_err() {
    // No regression on the nested path: still fires the CRITICAL bail.
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::create_dir(temp.path().join(".loom")).unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join(".loom").join("work")).unwrap();

    assert!(validate_work_dir_state(temp.path()).is_err());
}

#[test]
fn test_is_work_dir_git_ignored() {
    let temp = TempDir::new().unwrap();

    // No gitignore
    assert!(!is_work_dir_git_ignored(temp.path()));

    // With proper ignore
    fs::write(temp.path().join(".gitignore"), ".loom/work/\n.loom/work\n").unwrap();
    assert!(is_work_dir_git_ignored(temp.path()));
}

#[test]
fn test_is_work_dir_git_ignored_legacy() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".work")).unwrap();
    fs::write(temp.path().join(".gitignore"), ".work/\n.work\n").unwrap();
    assert!(is_work_dir_git_ignored(temp.path()));
}

#[test]
fn test_is_work_dir_git_ignored_false_when_pair_does_not_match_layout() {
    let temp = TempDir::new().unwrap();
    // Legacy layout on disk, but the .gitignore only carries the nested
    // pair — must not match.
    fs::create_dir_all(temp.path().join(".work")).unwrap();
    fs::write(temp.path().join(".gitignore"), ".loom/work/\n.loom/work\n").unwrap();
    assert!(!is_work_dir_git_ignored(temp.path()));
}

/// Initialise `temp` as a git repository isolated from the host's git config
fn git_repo(temp: &Path) {
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(temp)
            .env("GIT_CONFIG_GLOBAL", temp.join("missing-global"))
            .env("GIT_CONFIG_SYSTEM", temp.join("missing-system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    let excludes = temp.join("no-global-excludes");
    run(&["config", "core.excludesFile", excludes.to_str().unwrap()]);
}

#[test]
fn a_bare_loom_rule_ignores_the_state_dir_in_a_git_repo() {
    let temp = TempDir::new().unwrap();
    git_repo(temp.path());
    fs::write(temp.path().join(".gitignore"), ".loom\n").unwrap();
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    assert!(is_work_dir_git_ignored(temp.path()));
}

#[test]
fn an_unlisted_state_dir_in_a_git_repo_is_not_ignored() {
    let temp = TempDir::new().unwrap();
    git_repo(temp.path());
    fs::write(temp.path().join(".gitignore"), "node_modules/\n").unwrap();
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    assert!(!is_work_dir_git_ignored(temp.path()));
}

#[test]
fn a_worktrees_glob_rule_counts_in_a_git_repo() {
    let temp = TempDir::new().unwrap();
    git_repo(temp.path());
    fs::write(temp.path().join(".gitignore"), ".worktrees/*\n").unwrap();
    assert!(is_worktrees_git_ignored(temp.path()));
}

#[test]
fn a_nested_temp_dir_does_not_borrow_an_outer_repos_rules() {
    let outer = TempDir::new().unwrap();
    git_repo(outer.path());
    fs::write(outer.path().join(".gitignore"), ".loom\n").unwrap();
    let inner = outer.path().join("inner");
    fs::create_dir_all(inner.join(".loom/work")).unwrap();
    fs::write(inner.join(".gitignore"), "node_modules/\n").unwrap();
    assert!(!is_work_dir_git_ignored(&inner));
}
