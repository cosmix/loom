//! Client for the trusted PostToolUse completion transition.

use crate::daemon::{read_message, read_user_token, write_message, Request, Response};
use anyhow::{bail, Context, Result};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub(super) const BROKER_ENV: &str = "LOOM_CONTROL_BROKER";
pub(super) const VERIFIED_MARKER: &str = "LOOM_CONTROL_VERIFICATION_PASSED";

pub(super) fn broker_requested() -> bool {
    std::env::var_os(BROKER_ENV).as_deref() == Some(std::ffi::OsStr::new("1"))
}

pub(super) fn send_completion(stage_id: &str, session_id: &str, work_dir: &Path) -> Result<()> {
    let auth_token = read_user_token(work_dir)
        .context("trusted completion broker could not read .work/user.token")?;
    let request = Request::CompleteStage {
        auth_token,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        nonce: uuid::Uuid::new_v4().simple().to_string(),
    };
    let socket_path = work_dir.join("orchestrator.sock");
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("failed to connect to daemon at {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .context("failed to set completion broker socket timeout")?;
    write_message(&mut stream, &request).context("failed to send CompleteStage request")?;
    let response: Response = read_message(&mut stream).context("failed to read daemon response")?;
    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => bail!("daemon refused completion: {message}"),
        Response::AuthenticationFailed => bail!("daemon rejected completion broker credential"),
        other => bail!("unexpected completion broker response: {other:?}"),
    }
}
