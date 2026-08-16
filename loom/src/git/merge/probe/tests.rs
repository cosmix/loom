use super::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "test@example.com"]);
    git_ok(root, &["config", "user.name", "Test"]);
    fs::write(root.join(".git/info/exclude"), ".work/\n").unwrap();
    fs::write(root.join("file.txt"), "base\n").unwrap();
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-m", "base"]);
    fs::create_dir(root.join(".work")).unwrap();
    temp
}

fn create_branch(root: &Path, name: &str, contents: &str) {
    git_ok(root, &["checkout", "-b", name]);
    fs::write(root.join("file.txt"), contents).unwrap();
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-m", name]);
    git_ok(root, &["checkout", "main"]);
}

#[test]
fn probe_reports_clean_merge_and_restores_checkout() {
    let temp = init_repo();
    let root = temp.path();
    git_ok(root, &["checkout", "-b", "feature"]);
    fs::write(root.join("new.txt"), "feature\n").unwrap();
    git_ok(root, &["add", "new.txt"]);
    git_ok(root, &["commit", "-m", "feature"]);
    git_ok(root, &["checkout", "main"]);

    let result = get_conflicting_files_from_status("feature", "main", root, &root.join(".work"));
    assert_eq!(result.unwrap(), MergeProbeOutcome::Clean);
    assert_eq!(checkout_reference(root).unwrap(), "main");
    assert!(!merge_head_exists_strict(root).unwrap());
}

#[test]
fn probe_reports_genuine_conflicts_and_restores_checkout() {
    let temp = init_repo();
    let root = temp.path();
    create_branch(root, "feature", "feature\n");
    fs::write(root.join("file.txt"), "main\n").unwrap();
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-m", "main"]);

    let result = get_conflicting_files_from_status("feature", "main", root, &root.join(".work"));
    assert_eq!(
        result.unwrap(),
        MergeProbeOutcome::Conflicts(vec!["file.txt".to_string()])
    );
    assert_eq!(checkout_reference(root).unwrap(), "main");
    assert!(!merge_head_exists_strict(root).unwrap());
}

#[test]
fn dirty_repository_is_an_infrastructure_failure_without_mutation() {
    let temp = init_repo();
    let root = temp.path();
    create_branch(root, "feature", "feature\n");
    fs::write(root.join("file.txt"), "uncommitted\n").unwrap();

    let result = get_conflicting_files_from_status("feature", "main", root, &root.join(".work"));
    assert!(matches!(
        result,
        Err(MergeProbeError::Infrastructure {
            operation: "cleanliness check",
            ..
        })
    ));
    assert_eq!(checkout_reference(root).unwrap(), "main");
    assert_eq!(
        fs::read_to_string(root.join("file.txt")).unwrap(),
        "uncommitted\n"
    );
}

#[test]
fn untracked_files_do_not_block_the_probe() {
    let temp = init_repo();
    let root = temp.path();
    git_ok(root, &["checkout", "-b", "feature"]);
    fs::write(root.join("new.txt"), "feature\n").unwrap();
    git_ok(root, &["add", "new.txt"]);
    git_ok(root, &["commit", "-m", "feature"]);
    git_ok(root, &["checkout", "main"]);
    fs::write(root.join("scratch-notes.md"), "not tracked\n").unwrap();

    let result = get_conflicting_files_from_status("feature", "main", root, &root.join(".work"));
    assert_eq!(result.unwrap(), MergeProbeOutcome::Clean);
    assert!(root.join("scratch-notes.md").exists());
    assert_eq!(checkout_reference(root).unwrap(), "main");
    assert!(!merge_head_exists_strict(root).unwrap());
}

#[test]
fn invalid_source_ref_is_not_reported_as_a_clean_merge() {
    let temp = init_repo();
    let root = temp.path();

    let result = get_conflicting_files_from_status("missing", "main", root, &root.join(".work"));
    assert!(matches!(
        result,
        Err(MergeProbeError::Infrastructure {
            operation: "merge",
            ..
        })
    ));
    assert_eq!(checkout_reference(root).unwrap(), "main");
}

#[cfg(unix)]
#[test]
fn checkout_hook_failure_is_not_reported_as_clean() {
    use std::os::unix::fs::PermissionsExt;

    let temp = init_repo();
    let root = temp.path();
    git_ok(root, &["branch", "target"]);
    let hook = root.join(".git/hooks/post-checkout");
    fs::write(
        &hook,
        "#!/bin/sh\nif [ \"$(git symbolic-ref --quiet --short HEAD)\" = \"target\" ]; then\n  exit 23\nfi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let result = get_conflicting_files_from_status("main", "target", root, &root.join(".work"));
    assert!(matches!(
        result,
        Err(MergeProbeError::Infrastructure {
            operation: "target checkout",
            ..
        })
    ));
    assert_eq!(checkout_reference(root).unwrap(), "main");
}

#[cfg(unix)]
#[test]
fn failed_checkout_restoration_is_typed_and_propagated() {
    use std::os::unix::fs::PermissionsExt;

    let temp = init_repo();
    let root = temp.path();
    create_branch(root, "feature", "feature\n");
    git_ok(root, &["branch", "target"]);
    let hook = root.join(".git/hooks/post-checkout");
    fs::write(
        &hook,
        "#!/bin/sh\nif [ \"$(git symbolic-ref --quiet --short HEAD)\" = \"target\" ]; then\n  git branch -D main >/dev/null 2>&1\nfi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let error = get_conflicting_files_from_status("feature", "target", root, &root.join(".work"))
        .unwrap_err();

    assert!(matches!(error, MergeProbeError::Restoration { .. }));
    assert_eq!(checkout_reference(root).unwrap(), "target");
    assert!(!merge_head_exists_strict(root).unwrap());
}

#[test]
fn failed_merge_abort_is_typed_and_propagated() {
    let temp = init_repo();
    let root = temp.path();
    create_branch(root, "feature", "feature\n");
    fs::write(root.join("file.txt"), "main\n").unwrap();
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-m", "main"]);
    let merge = git(root, &["merge", "--no-ff", "feature"]);
    assert!(!merge.status.success());
    assert!(merge_head_exists_strict(root).unwrap());

    let index_lock = root.join(".git/index.lock");
    fs::write(&index_lock, "locked\n").unwrap();
    let mut guard = RepositoryStateGuard::new(root, "main".to_string());
    let error = guard.restore().unwrap_err();
    fs::remove_file(index_lock).unwrap();

    assert!(matches!(
        error,
        MergeProbeError::Restoration { ref details }
            if details.contains("merge abort failed")
    ));
}
