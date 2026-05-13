//! Thin CLI client for `loom stage dispute-criteria`.
//!
//! This command no longer mutates stage state directly. It serialises
//! the dispute into a structured `Request::DisputeCriteria` and sends
//! it over the daemon's Unix socket. The daemon writes
//! `.work/disputes/<stage>/<n>/request.md`, transitions the stage to
//! `NeedsAdjudication`, and returns an allocated id.
//!
//! See `loom/src/daemon/server/dispute.rs` for the server-side handler
//! and `loom/src/models/dispute.rs` for the on-disk schema.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

const FAILURE_OUTPUT_MAX_BYTES: usize = 4096;

/// Dispute an acceptance criterion via the daemon RPC.
///
/// `failure_output_path` is optional — when set, the file is read,
/// truncated to 4KB on a UTF-8 char boundary, and shipped as the
/// `failure_output` field of the request.
pub fn dispute_criteria(
    stage_id: String,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output_path: Option<PathBuf>,
) -> Result<()> {
    use crate::daemon::{read_message, read_user_token, write_message, Request, Response};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let work_dir = Path::new(".work");

    let failure_output = match failure_output_path {
        Some(path) => Some(load_and_truncate_failure_output(&path)?),
        None => None,
    };

    let auth_token = read_user_token(work_dir)
        .context("Failed to read .work/user.token for daemon authentication")?;

    let req = Request::DisputeCriteria {
        auth_token,
        stage_id: stage_id.clone(),
        criterion_index,
        reason: reason.clone(),
        evidence_commit,
        failure_output,
    };

    let socket_path = work_dir.join("orchestrator.sock");
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Failed to connect to daemon at {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .context("Failed to set socket read timeout")?;

    write_message(&mut stream, &req).context("Failed to send DisputeCriteria request")?;
    let response: Response = read_message(&mut stream).context("Failed to read daemon response")?;

    match response {
        Response::DisputeCreated { id } => {
            println!("Filed dispute #{id} for stage '{stage_id}' (criterion {criterion_index}).");
            println!("Reason: {reason}");
            println!();
            println!(
                "The stage is now in NeedsAdjudication. The adjudicator will issue a \
                 verdict; run `loom status` to monitor."
            );
            Ok(())
        }
        Response::Error { message } => {
            bail!("Daemon refused dispute: {message}")
        }
        Response::AuthenticationFailed => {
            bail!("Daemon authentication failed — check .work/user.token")
        }
        other => bail!("Unexpected daemon response to DisputeCriteria: {other:?}"),
    }
}

/// Load `failure_output_path` and truncate the contents at the last
/// UTF-8 char boundary that fits within `FAILURE_OUTPUT_MAX_BYTES`
/// (4KB). Avoids the multi-byte panic documented in
/// knowledge/mistakes.md § "String Handling: UTF-8 Truncation Panic".
fn load_and_truncate_failure_output(path: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read failure_output file: {}", path.display()))?;
    Ok(truncate_to_byte_limit(&raw, FAILURE_OUTPUT_MAX_BYTES))
}

fn truncate_to_byte_limit(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut acc = String::new();
    let mut byte_count = 0;
    for ch in s.chars() {
        let ch_len = ch.len_utf8();
        if byte_count + ch_len > max_bytes {
            break;
        }
        byte_count += ch_len;
        acc.push(ch);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn truncate_failure_output_at_4kb() {
        // 10KB of ASCII — easy byte/char correspondence.
        let mut file = NamedTempFile::new().unwrap();
        let big = "a".repeat(10_000);
        file.write_all(big.as_bytes()).unwrap();
        let truncated = load_and_truncate_failure_output(file.path()).unwrap();
        assert!(truncated.len() <= 4096, "got {} bytes", truncated.len());
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_failure_output_handles_multibyte_chars() {
        // Construct content that would split a multibyte char if naively sliced.
        // '🌀' is 4 bytes UTF-8; many copies push past 4KB exactly between bytes.
        let mut s = String::new();
        while s.len() < 5_000 {
            s.push('🌀');
        }
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(s.as_bytes()).unwrap();
        let truncated = load_and_truncate_failure_output(file.path()).unwrap();
        assert!(truncated.len() <= 4096);
        // Must still be valid UTF-8 ending on a char boundary.
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_failure_output_passthrough_under_limit() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        let truncated = load_and_truncate_failure_output(file.path()).unwrap();
        assert_eq!(truncated, "hello world");
    }
}
