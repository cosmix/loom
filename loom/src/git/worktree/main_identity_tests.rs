use super::*;
use crate::verify::contracts::test_support::contract_worktree;
use tempfile::TempDir;

fn setup() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let linked = contract_worktree(&repo, "stage");
    // Exercise initialization and legacy migration from the fixture's initial state.
    let anchor = repo.join(".loom/work").join(FILE);
    if anchor.exists() {
        fs::remove_file(anchor).unwrap();
    }
    (tmp, repo, linked)
}

#[test]
fn restart_uses_captured_identity_and_refuses_changed_directory() {
    let (_tmp, repo, _) = setup();
    let state = repo.join(".loom/work");
    fs::create_dir_all(&state).unwrap();
    capture_main_identity(&repo, &state).unwrap();
    require_main_identity(&repo).unwrap();
    fs::rename(repo.join(".git"), repo.join("old-git")).unwrap();
    fs::create_dir(repo.join(".git")).unwrap();
    assert!(require_main_identity(&repo).is_err());
}

#[test]
fn unanchored_existing_state_requires_operator_recovery() {
    let (_tmp, repo, _) = setup();
    fs::create_dir_all(repo.join(".loom/work/sessions")).unwrap();
    fs::write(repo.join(".loom/work/sessions/s.md"), "session").unwrap();
    let error = require_main_identity(&repo).unwrap_err().to_string();
    assert!(error.contains("Stop all sessions"), "{error}");
    assert!(repo.join(".loom/work/sessions/s.md").exists());
}

#[test]
fn legacy_session_without_config_cannot_fall_back_to_discovery() {
    let (_tmp, repo, _) = setup();
    // The fixture's empty nested state is removed so only the legacy state exists.
    fs::remove_dir_all(repo.join(".loom/work")).unwrap();
    let sessions = repo.join(".work/sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(sessions.join("s.md"), "session").unwrap();
    assert!(identity_for(&repo).is_err());
    assert!(sessions.join("s.md").exists());
}

/// What the prompt and context hooks leave in a repository `loom init` never ran in.
fn write_hook_state(state: &Path) {
    for dir in ["context", "memory", "telemetry"] {
        fs::create_dir_all(state.join(dir)).unwrap();
    }
    fs::write(state.join("memory/ad-hoc.md"), "note\n").unwrap();
    fs::write(state.join("telemetry/events.jsonl"), "{}\n").unwrap();
}

#[test]
fn hook_created_state_without_init_resolves_by_discovery() {
    let (_tmp, repo, _) = setup();
    fs::remove_dir_all(repo.join(".loom/work")).unwrap();
    write_hook_state(&repo.join(".loom/work"));
    let git = repo.join(".git").canonicalize().unwrap();
    assert_eq!(identity_for(&repo).unwrap(), (git.clone(), git));
}

#[test]
fn initialized_state_without_its_identity_fails_closed() {
    let (_tmp, repo, _) = setup();
    let state = repo.join(".loom/work");
    write_hook_state(&state);
    fs::write(state.join("config.toml"), "[plan]\nplan_id = \"p\"\n").unwrap();
    let error = format!("{:#}", identity_for(&repo).unwrap_err());
    assert!(error.contains("Stop all sessions"), "{error}");
}

#[test]
fn hook_created_nested_state_does_not_hide_legacy_sessions() {
    let (_tmp, repo, _) = setup();
    fs::remove_dir_all(repo.join(".loom/work")).unwrap();
    write_hook_state(&repo.join(".loom/work"));
    let sessions = repo.join(".work/sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(sessions.join("s.md"), "session").unwrap();
    assert!(identity_for(&repo).is_err());
}

#[test]
fn forged_anchor_symlink_is_refused_and_preserved() {
    let (tmp, repo, _) = setup();
    let state = repo.join(".loom/work");
    fs::create_dir_all(&state).unwrap();
    let foreign = tmp.path().join("forged.json");
    fs::write(&foreign, "{} ").unwrap();
    std::os::unix::fs::symlink(&foreign, state.join(FILE)).unwrap();
    assert!(require_main_identity(&repo).is_err());
    assert!(capture_main_identity(&repo, &state).is_err());
    assert_eq!(fs::read_to_string(foreign).unwrap(), "{} ");
}

#[test]
fn linked_checkout_uses_captured_registry_after_pointer_forgery() {
    let (tmp, repo, linked) = setup();
    let state = linked.join(".loom/work");
    fs::create_dir_all(&state).unwrap();
    capture_main_identity(&linked, &state).unwrap();
    let foreign = tmp.path().join("foreign/worktrees/forged");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(
        foreign.join("gitdir"),
        linked.join(".git").display().to_string(),
    )
    .unwrap();
    fs::write(foreign.join("commondir"), "../..").unwrap();
    fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", foreign.display()),
    )
    .unwrap();
    let (git, common) = identity_for(&linked).unwrap();
    assert_eq!(
        git,
        repo.join(".git/worktrees/stage").canonicalize().unwrap()
    );
    assert_eq!(common, repo.join(".git").canonicalize().unwrap());
}
