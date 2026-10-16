use std::env;
use tempfile::TempDir;

use super::*;

/// An empty ancestor `.git` must not become a repository or acquire work
/// state when `note` walks upward from a nested directory. Both the impostor
/// and the starting directory belong to this fixture.
#[test]
#[serial]
fn note_does_not_adopt_an_impostor_git_dir_at_the_temp_root() {
    let temp_root = TempDir::new().unwrap();
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    std::fs::create_dir(temp_root.path().join(".git")).unwrap();
    let plain_dir = temp_root.path().join("plain");
    std::fs::create_dir(&plain_dir).unwrap();
    env::set_current_dir(&plain_dir).unwrap();

    let result = note("should not be recorded".to_string(), Vec::new(), None);

    assert!(
        result.is_err(),
        "an impostor .git at the temp root must not be adopted as a repo root"
    );
    assert!(!temp_root.path().join(".loom").join("work").exists());
}
