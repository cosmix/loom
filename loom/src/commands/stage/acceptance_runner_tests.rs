use super::*;
use crate::models::stage::Stage;
use serial_test::serial;
use tempfile::TempDir;

#[test]
fn failure_guidance_names_every_route() {
    let text = failure_guidance("s1");
    for route in [
        "loom stage dispute-criteria s1 --criterion-index",
        "--field wiring --criterion-index",
        "--field wiring-tests --criterion-index",
        "loom stage block s1",
    ] {
        assert!(text.contains(route), "missing `{route}` in:\n{text}");
    }
}

#[test]
fn test_resolve_acceptance_dir_dot_uses_worktree_root() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    let result = resolve_acceptance_dir(Some(worktree_root), Some(".")).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), worktree_root.to_path_buf());
}

#[test]
fn test_resolve_acceptance_dir_subdir_uses_worktree_root_joined() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create the subdirectory
    let subdir_path = worktree_root.join("loom");
    std::fs::create_dir_all(&subdir_path).unwrap();

    let result = resolve_acceptance_dir(Some(worktree_root), Some("loom")).unwrap();

    assert!(result.is_some());
    // Canonicalize for comparison
    let expected = subdir_path.canonicalize().unwrap();
    assert_eq!(result.unwrap(), expected);
}

#[test]
fn test_resolve_acceptance_dir_missing_subdir_returns_error() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Don't create the subdirectory - should return error
    let result = resolve_acceptance_dir(Some(worktree_root), Some("nonexistent"));

    assert!(result.is_err());
}

#[test]
fn test_resolve_acceptance_dir_none_working_dir_uses_worktree_root() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    let result = resolve_acceptance_dir(Some(worktree_root), None).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), worktree_root.to_path_buf());
}

#[test]
fn test_resolve_acceptance_dir_no_worktree_returns_none() {
    let result = resolve_acceptance_dir(None, Some(".")).unwrap();

    assert!(result.is_none());
}

#[test]
fn test_resolve_acceptance_dir_nested_subdir() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_root = temp_dir.path();

    // Create a nested subdirectory
    let subdir_path = worktree_root.join("packages/core");
    std::fs::create_dir_all(&subdir_path).unwrap();

    let result = resolve_acceptance_dir(Some(worktree_root), Some("packages/core")).unwrap();

    assert!(result.is_some());
    // Canonicalize for comparison
    let expected = subdir_path.canonicalize().unwrap();
    assert_eq!(result.unwrap(), expected);
}

#[test]
#[serial]
fn test_resolve_stage_execution_paths_prefers_stage_worktree_when_cwd_mismatched() {
    let temp_dir = TempDir::new().unwrap();
    let expected_root = temp_dir.path().join(".worktrees/expected-stage");
    let other_root = temp_dir.path().join(".worktrees/other-stage");

    real_worktree(temp_dir.path(), "expected-stage");
    git(
        temp_dir.path(),
        &[
            "worktree",
            "add",
            "-b",
            "loom/other-stage",
            ".worktrees/other-stage",
        ],
    );

    let stage = Stage {
        id: "expected-stage".into(),
        worktree: Some("expected-stage".to_string()),
        ..Default::default()
    };

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&other_root).unwrap();

    let resolved = resolve_stage_execution_paths(&stage).unwrap();

    std::env::set_current_dir(original_dir).unwrap();

    assert_eq!(
        resolved.worktree_root.unwrap().canonicalize().unwrap(),
        expected_root.canonicalize().unwrap()
    );
}

#[test]
#[serial]
fn test_resolve_stage_execution_paths_uses_cwd_when_matching_stage_worktree() {
    let temp_dir = TempDir::new().unwrap();
    let expected_root = temp_dir.path().join(".worktrees/expected-stage");
    real_worktree(temp_dir.path(), "expected-stage");

    let stage = Stage {
        id: "expected-stage".into(),
        worktree: Some("expected-stage".to_string()),
        ..Default::default()
    };

    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&expected_root).unwrap();

    let resolved = resolve_stage_execution_paths(&stage).unwrap();

    std::env::set_current_dir(original_dir).unwrap();

    assert_eq!(
        resolved.worktree_root.unwrap().canonicalize().unwrap(),
        expected_root.canonicalize().unwrap()
    );
}

#[test]
#[serial]
fn missing_assigned_real_worktree_never_runs_commands_in_another_stage() {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "--allow-empty",
            "-qm",
            "init",
        ],
    );
    git(root, &["worktree", "add", "-b", "loom/a", ".worktrees/a"]);
    git(root, &["worktree", "add", "-b", "loom/b", ".worktrees/b"]);
    std::fs::remove_dir_all(root.join(".worktrees/a")).unwrap();
    struct CwdGuard(std::path::PathBuf);
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).unwrap();
        }
    }
    let _cwd = CwdGuard(std::env::current_dir().unwrap());
    std::env::set_current_dir(root.join(".worktrees/b")).unwrap();
    let stage = Stage {
        id: "a".into(),
        worktree: Some("a".into()),
        ..Default::default()
    };
    let result = resolve_stage_execution_paths(&stage).and_then(|paths| {
        std::process::Command::new("sh")
            .args(["-c", "touch marker"])
            .current_dir(paths.acceptance_dir.unwrap())
            .status()?;
        Ok(())
    });
    assert!(format!("{:#}", result.unwrap_err()).contains("Assigned worktree"));
    assert!(!root.join(".worktrees/b/marker").exists());
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn real_worktree(root: &std::path::Path, id: &str) {
    git(root, &["init", "-q"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "--allow-empty",
            "-qm",
            "init",
        ],
    );
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &format!("loom/{id}"),
            &format!(".worktrees/{id}"),
        ],
    );
}
