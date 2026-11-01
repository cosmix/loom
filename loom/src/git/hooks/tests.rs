//! Tests for pre-commit hook installation and chaining.

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

fn repo_with_hook(content: &str) -> (TempDir, std::path::PathBuf) {
    let repo = TempDir::new().unwrap();
    let hooks = repo.path().join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    fs::write(&hook, content).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    (repo, hook)
}

fn invoke(root: &Path, hook: &Path) -> std::process::Output {
    Command::new(hook)
        .arg("kept-argument")
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn test_extract_loom_section() {
    let content =
        "#!/bin/bash\n# LOOM_PRE_COMMIT_HOOK_START\necho test\n# LOOM_PRE_COMMIT_HOOK_END\n";
    let section = extract_loom_section(content);
    assert!(section.contains("LOOM_PRE_COMMIT_HOOK_START"));
    assert!(section.contains("echo test"));
    assert!(section.contains("LOOM_PRE_COMMIT_HOOK_END"));
}

#[test]
fn install_pre_commit_hook_refuses_outside_repo() {
    let temp = TempDir::new().unwrap();

    let result = install_pre_commit_hook(temp.path());

    assert!(result.is_err());
    assert!(!temp.path().join(".git").exists());
}

#[test]
fn test_install_pre_commit_hook_new() {
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    fs::create_dir_all(&git_dir).unwrap();

    let result = install_pre_commit_hook(temp.path());
    assert!(result.is_ok());
    assert!(result.unwrap()); // Should return true for new install

    let hook_path = git_dir.join("hooks/pre-commit");
    assert!(hook_path.exists());

    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains(LOOM_HOOK_START_MARKER));
}

#[test]
fn test_install_pre_commit_hook_idempotent() {
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    fs::create_dir_all(&git_dir).unwrap();

    // First install
    let result1 = install_pre_commit_hook(temp.path());
    assert!(result1.is_ok());
    assert!(result1.unwrap());

    // Second install should return false (no changes)
    let result2 = install_pre_commit_hook(temp.path());
    assert!(result2.is_ok());
    assert!(!result2.unwrap());
}

#[test]
fn test_is_pre_commit_hook_installed() {
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    fs::create_dir_all(&git_dir).unwrap();

    // Not installed initially
    assert!(!is_pre_commit_hook_installed(temp.path()));

    // Install and check
    install_pre_commit_hook(temp.path()).unwrap();
    assert!(is_pre_commit_hook_installed(temp.path()));
}

#[test]
fn current_hook_with_removed_execute_bit_is_repaired() {
    let (repo, hook) = repo_with_hook("#!/bin/sh\nexit 0\n");
    install_pre_commit_hook(repo.path()).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!is_pre_commit_hook_installed(repo.path()));
    assert!(install_pre_commit_hook(repo.path()).unwrap());
    assert!(is_pre_commit_hook_installed(repo.path()));
    assert_eq!(
        fs::metadata(&hook).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[test]
fn foreign_shell_hook_runs_after_loom_and_keeps_its_exit_status() {
    let original = "#!/bin/sh\nprintf '%s' \"$1\" > .git/foreign-ran\nexit 7\n";
    let (repo, hook) = repo_with_hook(original);
    install_pre_commit_hook(repo.path()).unwrap();
    assert_eq!(
        fs::read_to_string(hook.with_file_name(FOREIGN_HOOK_NAME)).unwrap(),
        original
    );
    let output = invoke(repo.path(), &hook);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        fs::read_to_string(repo.path().join(".git/foreign-ran")).unwrap(),
        "kept-argument"
    );
}

#[test]
fn early_exit_zero_in_foreign_hook_still_runs_after_loom() {
    let original = "#!/bin/sh\nprintf reached > .git/foreign-ran\nexit 0\n";
    let (repo, hook) = repo_with_hook(original);
    install_pre_commit_hook(repo.path()).unwrap();
    assert!(invoke(repo.path(), &hook).status.success());
    assert_eq!(
        fs::read_to_string(repo.path().join(".git/foreign-ran")).unwrap(),
        "reached"
    );
    assert_eq!(
        fs::read_to_string(hook.with_file_name(FOREIGN_HOOK_NAME)).unwrap(),
        original
    );
}

#[test]
fn loom_rejects_a_forbidden_staged_path_before_foreign_hook() {
    let repo = TempDir::new().unwrap();
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(init.status.success());
    fs::create_dir_all(repo.path().join(".work")).unwrap();
    fs::write(repo.path().join(".work/blocked"), "x").unwrap();
    let add = Command::new("git")
        .args(["add", "-f", ".work/blocked"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(add.status.success());
    let hook = repo.path().join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\nprintf foreign > .git/foreign-ran\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    install_pre_commit_hook(repo.path()).unwrap();
    assert_eq!(invoke(repo.path(), &hook).status.code(), Some(1));
    assert!(!repo.path().join(".git/foreign-ran").exists());
}

#[test]
fn non_shell_shebang_runs_unchanged_via_the_chain() {
    let original = "#!/bin/echo\nnon-shell original\n";
    let (repo, hook) = repo_with_hook(original);
    install_pre_commit_hook(repo.path()).unwrap();
    assert_eq!(
        fs::read_to_string(hook.with_file_name(FOREIGN_HOOK_NAME)).unwrap(),
        original
    );
    let output = invoke(repo.path(), &hook);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("kept-argument"));
}

#[test]
fn ambiguous_foreign_hook_is_left_untouched() {
    let original = "exit 0\n";
    let (repo, hook) = repo_with_hook(original);
    let error = install_pre_commit_hook(repo.path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("executable shebang"), "{error}");
    assert_eq!(fs::read_to_string(hook).unwrap(), original);
}
