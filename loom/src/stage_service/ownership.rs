use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::Stage;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::load_stage;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;
pub(crate) fn session_owns_stage_as(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    allowed: &[SessionType],
) -> Result<()> {
    // Both ids arrive unvalidated from the wire and both are turned into
    // paths below, so traversal shapes have to die before any file is touched.
    crate::validation::validate_id(stage_id).context("invalid stage id")?;
    crate::validation::validate_id(session_id).context("invalid session id")?;

    let stage = load_stage(stage_id, work_dir)?;
    if stage.session.as_deref() != Some(session_id) {
        bail!("session '{session_id}' is not active for stage '{stage_id}'");
    }

    let relative = PathBuf::from("sessions").join(format!("{session_id}.md"));
    let content =
        crate::fs::safe_read::read_to_string_bounded(work_dir, &relative, MAX_SESSION_FILE_BYTES)
            .with_context(|| format!("failed to read active session '{session_id}'"))?;
    let session: Session =
        parse_from_markdown(&content, "session").context("invalid active session file")?;
    if !owns_stage_as(&stage, &session, stage_id, session_id, allowed) {
        bail!("request does not match the active running stage session");
    }
    Ok(())
}

/// The ownership test on records already read: the stage names the session,
/// and the session's record is that session, of an allowed type, running for
/// that stage. A caller that reads the records itself keeps a read failure
/// apart from a mismatch, which `session_owns_stage_as` reports as one error.
pub(crate) fn owns_stage_as(
    stage: &Stage,
    session: &Session,
    stage_id: &str,
    session_id: &str,
    allowed: &[SessionType],
) -> bool {
    stage.session.as_deref() == Some(session_id)
        && session.id == session_id
        && session.stage_id.as_deref() == Some(stage_id)
        && allowed.contains(&session.session_type)
        && session.status == SessionStatus::Running
}
