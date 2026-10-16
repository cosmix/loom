use super::kernel;
use crate::models::session::SessionType;
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub(super) fn authorize(
    work_dir: &Path,
    stage: &str,
    session: &str,
    cwd: &Path,
    peer: Option<u32>,
) -> Result<()> {
    crate::validation::validate_id(stage)?;
    crate::validation::validate_id(session)?;
    let peer = peer.context("no kernel peer identity")?;
    ensure!(kernel::sandboxed(peer), "peer is unsandboxed");
    let root = work_dir.canonicalize()?;
    protected_records(&root, stage, session, peer)?;
    ensure!(
        crate::daemon::caller_is_inside_session(&root, session, peer),
        "peer is outside the live session"
    );
    crate::stage_service::ownership::session_owns_stage_as(
        &root,
        stage,
        session,
        &[
            SessionType::Stage,
            SessionType::Contract,
            SessionType::Knowledge,
        ],
    )?;
    validate_capsule(&root, session)?;
    validate_cwd(&root, stage, cwd)
}

fn protected_records(root: &Path, stage: &str, session: &str, peer: u32) -> Result<()> {
    let relative = PathBuf::from(format!("sessions/{session}.md"));
    let content = crate::fs::safe_read::read_to_string_bounded(root, &relative, 1024 * 1024)?;
    let record: crate::models::session::Session =
        crate::parser::frontmatter::parse_from_markdown(&content, "session")?;
    let (_, pid_key) =
        crate::orchestrator::terminal::native::NativeBackend::window_title_and_pid_key(&record)
            .context("session has no process identity")?;
    let stage_path = crate::fs::stage_files::find_stage_file(&root.join("stages"), stage)?
        .context("missing stage record")?;
    ensure!(
        kernel::protected(peer, &stage_path),
        "writable stage evidence"
    );
    let paths = [
        PathBuf::new(),
        PathBuf::from("sessions"),
        PathBuf::from("stages"),
        PathBuf::from("capsules"),
        PathBuf::from("pids"),
        PathBuf::from(format!("pids/{pid_key}.pid")),
        PathBuf::from(format!("sessions/{session}.md")),
        PathBuf::from(format!("capsules/{session}.settings.json")),
    ];
    for path in paths {
        ensure!(
            kernel::protected(peer, &root.join(path)),
            "writable session evidence"
        );
    }
    Ok(())
}

pub(super) fn validate_capsule(root: &Path, session: &str) -> Result<()> {
    let relative = PathBuf::from("capsules").join(format!("{session}.settings.json"));
    let bytes = crate::fs::safe_read::read_bounded(root, &relative, 1024 * 1024)?;
    let capsule: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        capsule
            .pointer("/sandbox/enabled")
            .and_then(|v| v.as_bool())
            == Some(true),
        "capsule sandbox is disabled"
    );
    Ok(())
}

pub(super) fn validate_cwd(root: &Path, stage: &str, cwd: &Path) -> Result<()> {
    let stage = crate::verify::transitions::load_stage(stage, root)?;
    let workspace = crate::fs::work_dir::WorkDir::new(root)?;
    let repo = workspace
        .repo_root()
        .context("no recorded repository root")?;
    let worktree = crate::git::worktree::stage_worktree_path(&stage, repo)?.canonicalize()?;
    ensure!(
        cwd.canonicalize()?.starts_with(worktree),
        "working directory is outside the stage worktree"
    );
    Ok(())
}
