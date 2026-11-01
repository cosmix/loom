use super::*;
use crate::orchestrator::terminal::tmux::socket::tests::{
    server_socket_dir, TmuxServerGuard, TmuxTmpDirGuard,
};
use serial_test::serial;
use tempfile::TempDir;

fn start_server(name: &str, session: &str) {
    let output = super::super::run_tmux_control(
        &["-L", name, "new-session", "-d", "-s", session],
        super::super::TMUX_SPAWN_TIMEOUT,
        "start viewer retirement test server",
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[serial]
fn retirement_only_targets_this_repo_viewer() {
    if which::which("tmux").is_err() {
        return;
    }
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();
    let work_a = repo_a.path().join(".loom/work");
    let work_b = repo_b.path().join(".loom/work");
    fs::create_dir_all(&work_a).unwrap();
    fs::create_dir_all(&work_b).unwrap();
    let a = viewer_socket_name(&work_a);
    let b = viewer_socket_name(&work_b);
    let _a_guard = TmuxServerGuard::new(&a);
    let _b_guard = TmuxServerGuard::new(&b);
    let _other_guard = TmuxServerGuard::new("unrelated-viewer");
    start_server(&a, OVERVIEW_SESSION);
    start_server(&b, OVERVIEW_SESSION);
    start_server("unrelated-viewer", "other-session");

    assert!(retire_viewer(&work_a).unwrap());
    assert!(!socket_path_for(&a).exists());
    assert!(socket_path_for(&b).exists());
    assert!(socket_path_for("unrelated-viewer").exists());
    start_server(&a, "wrong-session");
    assert!(retire_viewer(&work_a).is_err());
    assert!(socket_path_for(&a).exists());
}

#[test]
#[serial]
fn retirement_rejects_non_socket_at_expected_name() {
    let socket_root = server_socket_dir();
    let _env = TmuxTmpDirGuard::set(socket_root.path());
    let repo = TempDir::new().unwrap();
    let work_dir = repo.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    let path = socket_path_for(&viewer_socket_name(&work_dir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "foreign data").unwrap();

    assert!(retire_viewer(&work_dir).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"foreign data");
}
