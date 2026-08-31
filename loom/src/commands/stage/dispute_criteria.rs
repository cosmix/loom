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

use crate::daemon::{
    current_session_id, try_send_request, user_credential, DaemonReach, Request, Response,
};
use crate::fs::stage_request::{append_to_spool, spool_path, spool_target_from_cwd, StageRequest};

const FAILURE_OUTPUT_MAX_BYTES: usize = 4096;

/// Dispute an acceptance criterion via the daemon RPC.
///
/// `failure_output_path` is optional — when set, the file is read,
/// truncated to 4KB on a UTF-8 char boundary, and shipped as the
/// `failure_output` field of the request.
///
/// The three ways the daemon can be reached call for three different answers.
/// With nothing listening the dispute simply cannot be filed: it is the daemon
/// that writes `.work/disputes/<stage>/<n>/request.md` and moves the stage to
/// `NeedsAdjudication`, so there is no local fallback to take here the way
/// `loom stage block` has one. `Unreachable` is different — it says nothing
/// about the daemon, only that this process may not use unix sockets — so it
/// queues the dispute rather than concluding there is no daemon (see
/// [`queue_dispute_request`]).
pub fn dispute_criteria(
    stage_id: String,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output_path: Option<PathBuf>,
) -> Result<()> {
    let work_dir = Path::new(".work");

    let failure_output = match failure_output_path {
        Some(path) => Some(load_and_truncate_failure_output(&path)?),
        None => None,
    };

    let req = build_request(
        work_dir,
        &stage_id,
        criterion_index,
        &reason,
        &evidence_commit,
        &failure_output,
    );

    match try_send_request(work_dir, &req)? {
        DaemonReach::Answered(response) => {
            handle_dispute_response(&stage_id, criterion_index, &reason, response)
        }
        DaemonReach::NotListening => bail!(
            "No daemon is listening on .work/orchestrator.sock, so the dispute cannot be \
             filed. The criterion stands until a daemon is running."
        ),
        DaemonReach::Unreachable => queue_dispute_request(
            &stage_id,
            StageRequest::Dispute {
                criterion_index,
                reason,
                evidence_commit,
                failure_output,
            },
        ),
    }
}

/// Build the RPC the daemon expects, cloning the fields so the caller keeps
/// its own copies for the spool fallback.
///
/// A missing or unreadable token is the NORMAL case here, not an error: the
/// agent that needs this command most is the one the sandbox denies the read
/// to (S-1). It names the session it is running inside instead, and the daemon
/// authorizes it by the connection.
fn build_request(
    work_dir: &Path,
    stage_id: &str,
    criterion_index: usize,
    reason: &str,
    evidence_commit: &Option<String>,
    failure_output: &Option<String>,
) -> Request {
    Request::DisputeCriteria {
        auth_token: user_credential(work_dir),
        stage_id: stage_id.to_string(),
        session_id: current_session_id(),
        criterion_index,
        reason: reason.to_string(),
        evidence_commit: evidence_commit.clone(),
        failure_output: failure_output.clone(),
    }
}

/// Queue a dispute for the daemon to file, for the caller that cannot reach it.
///
/// Queueing does not weaken the authorization the RPC path establishes: the
/// daemon still runs the same handler, still enforces the criterion-index and
/// dispute-budget checks, and still attributes the dispute to the worktree it
/// drained it from rather than to anything the request claims about itself.
fn queue_dispute_request(stage_id: &str, request: StageRequest) -> Result<()> {
    let worktree_root = spool_target_from_cwd()?;
    append_to_spool(&worktree_root, &request)?;

    println!("Queued a dispute for stage '{stage_id}' for the loom daemon to file.");
    println!("Queued at: {}", spool_path(&worktree_root).display());
    println!();
    // No id to print, and inventing one would be worse than saying so: ids are
    // allocated by the daemon at filing time, under the per-stage lock that
    // makes them sequential.
    println!(
        "There is no dispute id yet — the daemon allocates one when it files the dispute \
         on its next poll. Run `loom status` to watch the stage reach NeedsAdjudication."
    );
    Ok(())
}

/// Interpret the daemon's answer to a `DisputeCriteria` request. A live
/// daemon's refusal is authoritative and reported verbatim — there is no
/// local fallback to defer to (see the `NotListening` arm in
/// `dispute_criteria` above).
fn handle_dispute_response(
    stage_id: &str,
    criterion_index: usize,
    reason: &str,
    response: Response,
) -> Result<()> {
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
            bail!(
                "Daemon refused this dispute: it accepted no credential and could not confirm \
                 this process is running inside the session that owns stage '{stage_id}'. \
                 Check that the loom daemon is running and that this is that session"
            )
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
