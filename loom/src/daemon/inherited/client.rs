use super::{kernel, transport};
use crate::daemon::{read_message, socket_path, write_message, Request, Response};
use crate::process::{verify_process_identity, IdentityStatus};
use anyhow::{ensure, Context, Result};
use std::path::Path;

pub(super) fn available(cwd: &Path) -> Result<bool> {
    ensure!(
        kernel::sandboxed(std::process::id()),
        "caller is unsandboxed"
    );
    // These values route the request only. They are never evidence.
    let work_dir = std::env::var_os("LOOM_WORK_DIR").context("no state root")?;
    let stage = std::env::var("LOOM_STAGE_ID")?;
    let session = std::env::var("LOOM_SESSION_ID")?;
    validate(Path::new(&work_dir), &stage, &session, cwd)
}

pub(super) fn validate(work_dir: &Path, stage: &str, session: &str, cwd: &Path) -> Result<bool> {
    ensure!(
        kernel::sandboxed(std::process::id()),
        "caller is unsandboxed"
    );
    crate::validation::validate_id(stage)?;
    crate::validation::validate_id(session)?;
    let root = work_dir.canonicalize()?;
    for path in [&root, &root.join(crate::fs::daemon_state::LOCK_FILE)] {
        ensure!(
            kernel::protected(std::process::id(), path),
            "unprotected daemon identity"
        );
    }
    let mut stream = authenticated_stream(&root)?;
    write_message(
        &mut stream,
        &Request::ValidateInheritedConfinement {
            auth_token: "peer-identity".into(),
            stage_id: stage.into(),
            session_id: session.into(),
            working_dir: cwd.canonicalize()?,
        },
    )?;
    Ok(matches!(
        read_message::<Response, _>(&mut stream)?,
        Response::Ok
    ))
}

fn authenticated_stream(root: &Path) -> Result<transport::Connection> {
    let identity = crate::fs::daemon_state::read_recorded_lock_identity(root)
        .context("missing daemon identity")?;
    ensure!(
        verify_process_identity(identity) == IdentityStatus::VerifiedAlive,
        "stale daemon identity"
    );
    let stream = transport::connect(&socket_path(root))?;
    ensure!(stream.peer_pid() == Some(identity.pid), "wrong daemon peer");
    ensure!(
        verify_process_identity(identity) == IdentityStatus::VerifiedAlive,
        "daemon identity changed"
    );
    Ok(stream)
}
