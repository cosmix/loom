//! `loom init --clean` against a held target.

use super::warn_on_long_socket_path;
use super::{execute, prepare_workspace, stop_daemon_and_prune};
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::git::target_guard::test_support::held_repo;
use crate::git::target_guard::RECORD_FILE;
use crate::process::sandbox_probe::{skip_unless, skip_without_unix_socket};
use serial_test::serial;
use std::path::{Path, PathBuf};

/// Enters `dir` and returns to the previous directory on drop, a panic
/// included: `execute` works from the current directory.
struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(original)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

fn cyclic_plan(dir: &Path) -> PathBuf {
    use crate::plan::schema::{AcceptanceCriterion, LoomConfig, LoomMetadata, StageDefinition};
    let stage = |id: &str, dependency: &str| StageDefinition {
        id: id.into(),
        name: id.into(),
        working_dir: ".".into(),
        dependencies: vec![dependency.into()],
        acceptance: vec![AcceptanceCriterion::Simple("true".into())],
        ..Default::default()
    };
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages: vec![stage("a", "b"), stage("b", "a")],
            ..Default::default()
        },
    };
    let yaml = crate::yaml::to_string(&metadata).unwrap();
    let path = dir.join("cycle-plan.md");
    std::fs::write(
        &path,
        format!(
            "# Cycle\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
        ),
    )
    .unwrap();
    path
}

#[test]
#[serial]
fn init_clean_refuses_a_cyclic_plan_and_keeps_the_old_run() {
    let _home = ScratchHome::set();
    let repo = crate::git::target_guard::test_support::repo();
    std::fs::write(repo.work.join("config.toml"), "[plan]\n").unwrap();
    let state_file = repo.work.join("keep.txt");
    std::fs::write(&state_file, "state").unwrap();
    let worktree_file = repo.root.join(".worktrees/stage-a/keep.txt");
    std::fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    std::fs::write(&worktree_file, "work").unwrap();
    let plan = cyclic_plan(&repo.root);
    let _cwd = CwdGuard::enter(&repo.root);

    let error = execute(Some(plan), true, None, true).unwrap_err();

    assert!(
        format!("{error:#}").contains("Circular dependency detected"),
        "{error:#}"
    );
    assert!(state_file.is_file());
    assert!(worktree_file.is_file());
}

#[test]
#[serial]
fn init_clean_from_a_subdirectory_refuses_like_the_root_does() {
    let _home = ScratchHome::set();
    let (repo, _, _) = held_repo(false);
    let anchor = repo.work.join("main-checkout-identity.json");
    if anchor.exists() {
        std::fs::remove_file(anchor).unwrap();
    }
    let sessions = repo.work.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let session = sessions.join("existing.md");
    std::fs::write(&session, "preserve session").unwrap();
    let subdirectory = repo.root.join("src");
    std::fs::create_dir_all(&subdirectory).unwrap();
    let _cwd = CwdGuard::enter(&subdirectory);

    let error = execute(None, true, None, true).unwrap_err().to_string();

    assert!(error.contains("Stop all sessions"), "{error}");
    assert_eq!(
        std::fs::read_to_string(session).unwrap(),
        "preserve session"
    );
}

#[test]
#[serial]
fn prepare_workspace_bootstraps_a_repository_with_no_commits() {
    let repo = crate::test_git::TestRepo::builder()
        .branch("main")
        .identity("t", "t@t.com")
        .no_seed()
        .create();

    let branch = prepare_workspace(repo.path(), false, true, true).unwrap();

    assert_eq!(branch.as_deref(), Some("main"));
    assert_eq!(
        prepare_workspace(repo.path(), false, true, false).unwrap(),
        None
    );
}

#[test]
fn clean_refuses_before_stopping_anything_while_the_target_is_held() {
    let (repo, _accepted, _moved) = held_repo(true);

    let error = stop_daemon_and_prune(&repo.root, true).unwrap_err();

    assert!(error.to_string().contains("loom target status"), "{error}");
    assert!(repo.work.join("target-guard.json").is_file());
}

#[test]
#[serial]
fn init_clean_refuses_through_execute_and_deletes_nothing() {
    let (repo, _accepted, _moved) = held_repo(true);
    if !crate::git::worktree::main_identity_exists(&repo.work) {
        crate::git::worktree::capture_main_identity(&repo.root, &repo.work).unwrap();
    }
    let worktree_file = repo.root.join(".worktrees/stage-a/keep.txt");
    std::fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    std::fs::write(&worktree_file, "work").unwrap();
    let state_file = repo.work.join("keep.txt");
    std::fs::write(&state_file, "state").unwrap();
    let record = std::fs::read(repo.work.join(RECORD_FILE)).unwrap();
    let _cwd = CwdGuard::enter(&repo.root);

    let error = execute(None, true, None, true).unwrap_err().to_string();

    assert!(
        error.contains("the target branch main has a move loom did not accept"),
        "{error}"
    );
    assert!(error.contains("touches .claude/settings.json"), "{error}");
    assert_eq!(std::fs::read(repo.work.join(RECORD_FILE)).unwrap(), record);
    assert!(worktree_file.is_file());
    assert!(state_file.is_file());
}

#[test]
#[serial]
fn init_refuses_unanchored_existing_sessions_without_deleting_them() {
    let (repo, _, _) = held_repo(false);
    let anchor = repo.work.join("main-checkout-identity.json");
    if anchor.exists() {
        std::fs::remove_file(anchor).unwrap();
    }
    let sessions = repo.work.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let session = sessions.join("existing.md");
    std::fs::write(&session, "preserve session").unwrap();
    let _cwd = CwdGuard::enter(&repo.root);

    let error = execute(None, true, None, true).unwrap_err().to_string();
    assert!(error.contains("Stop all sessions"), "{error}");
    assert_eq!(
        std::fs::read_to_string(session).unwrap(),
        "preserve session"
    );
}

#[test]
fn a_work_root_whose_socket_path_cannot_fit_is_warned_about() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let short = root.join("w");
    let long = root.join("l".repeat(crate::daemon::SUN_PATH_MAX));
    std::fs::create_dir(&short).unwrap();
    std::fs::create_dir(&long).unwrap();
    let fits = crate::daemon::socket_path_fits(&short.join(crate::daemon::SOCKET_FILE));
    if skip_unless(
        fits,
        "a_work_root_whose_socket_path_cannot_fit_is_warned_about",
        "the temp dir is too long for a short work root's socket path to fit",
    ) {
        return;
    }

    let message = warn_on_long_socket_path(&long).expect("a long work root is warned about");

    assert!(message.contains(&long.display().to_string()));
    assert_eq!(warn_on_long_socket_path(&short), None);
}

/// `loom init --clean` proceeds past the viewer socket a dead tmux server left,
/// and the stale socket goes with the state it belonged to.
#[test]
#[serial]
fn init_clean_proceeds_past_a_dead_viewer_socket() {
    use crate::orchestrator::terminal::tmux::socket::tests::{server_socket_dir, TmuxTmpDirGuard};
    use crate::orchestrator::terminal::tmux::socket_path_for;
    use crate::orchestrator::terminal::tmux::viewer::viewer_socket_name;
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = crate::git::target_guard::test_support::repo();
    let socket = socket_path_for(&viewer_socket_name(&repo.work));
    if skip_without_unix_socket("init_clean_proceeds_past_a_dead_viewer_socket", &socket) {
        return;
    }
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());

    super::cleanup_previous_run(&repo.root, true).unwrap();

    assert!(std::fs::symlink_metadata(&socket).is_err());
    assert!(!repo.work.exists());
}
