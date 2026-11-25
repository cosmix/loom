//! Contract tests for the tmux-only terminal lane: a missing tmux names the
//! one fix, and no stale `[terminal]` setting routes a spawn around it.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::orchestrator::terminal::backend::TMUX_MISSING_MESSAGE;
use crate::orchestrator::terminal::{require_tmux, SessionBackend};

#[test]
fn missing_tmux_names_the_one_fix() {
    assert_eq!(
        TMUX_MISSING_MESSAGE,
        "Loom needs tmux on PATH to start sessions. Install tmux and retry."
    );
    let err = require_tmux(|| false).expect_err("a missing tmux must be an error");
    assert_eq!(err.to_string(), TMUX_MISSING_MESSAGE);
    assert!(require_tmux(|| true).is_ok(), "tmux on PATH must pass");
}

/// Every path under `root`, sorted, so a created PID or wrapper file shows.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read work dir") {
            let path = entry.expect("read work dir entry").path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            found.push(path);
        }
    }
    found.sort();
    found
}

fn assert_tmux_missing(kind: &str, result: anyhow::Result<Session>) {
    let err = match result {
        Ok(_) => panic!("{kind} spawned although tmux is missing"),
        Err(err) => err,
    };
    assert!(
        err.chain()
            .any(|cause| cause.to_string().contains(TMUX_MISSING_MESSAGE)),
        "{kind}: error chain lacks the tmux message: {err:#}"
    );
}

/// Calls all five spawn methods; `outside` holds the signal file, the
/// worktree path and serves as the repository root.
fn spawn_all(
    backend: &SessionBackend,
    outside: &Path,
) -> Vec<(&'static str, anyhow::Result<Session>)> {
    let signal = outside.join("signal.md");
    std::fs::write(&signal, "# Signal\n").expect("write signal");
    let mut stage = Stage::new("s".into(), None);
    stage.id = "s".into();
    let worktree = Worktree::new("s".into(), outside.join("wt"), "loom/s".into());
    let merge = Session::new_merge("loom/s".into(), "main".into());
    vec![
        (
            "knowledge",
            backend.spawn_knowledge_session(&stage, Session::new_knowledge("s"), &signal, outside),
        ),
        (
            "adjudication",
            backend.spawn_adjudication_session(
                &stage,
                Session::new_adjudication("s"),
                &signal,
                outside,
            ),
        ),
        (
            "stage",
            backend.spawn_session(&stage, &worktree, Session::new(), &signal),
        ),
        (
            "contract",
            backend.spawn_contract_session(&stage, &worktree, Session::new_contract("s"), &signal),
        ),
        (
            "merge",
            backend.spawn_merge_session_in_worktree(&stage, &worktree, merge, &signal),
        ),
    ]
}

#[test]
fn stale_native_setting_cannot_bypass_tmux() {
    let work = TempDir::new().expect("work dir");
    std::fs::write(
        work.path().join("config.toml"),
        "[terminal]\nbackend = \"native\"\n",
    )
    .expect("write config.toml");
    let backend = SessionBackend::with_tmux_probe(work.path().to_path_buf(), || false);
    let before = files_under(work.path());

    let outside = TempDir::new().expect("outside dir");
    for (kind, result) in spawn_all(&backend, outside.path()) {
        assert_tmux_missing(kind, result);
    }

    assert_eq!(
        files_under(work.path()),
        before,
        "a spawn wrote launch files before checking for tmux"
    );
}
