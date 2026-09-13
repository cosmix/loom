//! Thin CLI client for `loom stage dispute-criteria`.
//!
//! This command no longer mutates stage state directly. It serialises
//! the dispute into a structured `Request::DisputeCriteria` and sends
//! it over the daemon's Unix socket. The daemon writes
//! `<state-dir>/disputes/<stage>/<n>/request.md`, transitions the stage to
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
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink};
use crate::relay::RequestKind;

const FAILURE_OUTPUT_MAX_BYTES: usize = 4096;

/// Dispute an acceptance criterion.
///
/// Reads the process environment exactly once, then delegates to
/// `dispute_criteria_with_mode` — the seam tests drive directly with an
/// explicit [`RelayMode`] and an in-memory sink, since tests must never
/// mutate process-wide environment.
pub fn dispute_criteria(
    stage_id: String,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output_path: Option<PathBuf>,
) -> Result<()> {
    let failure_output = match failure_output_path {
        Some(path) => Some(load_and_truncate_failure_output(&path)?),
        None => None,
    };
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    dispute_criteria_with_mode(
        stage_id,
        criterion_index,
        reason,
        evidence_commit,
        failure_output,
        relay_mode,
        &cwd,
        &mut StdSink::default(),
    )
}

/// In Relay mode the request goes to the relay hook; otherwise the daemon
/// socket, as today (see [`dispute_via_socket`]).
#[allow(clippy::too_many_arguments)]
fn dispute_criteria_with_mode(
    stage_id: String,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        return dispute_via_relay(
            &stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
            &context,
            cwd,
            sink,
        );
    }
    dispute_via_socket(
        stage_id,
        criterion_index,
        reason,
        evidence_commit,
        failure_output,
    )
}

/// The three ways the daemon can be reached call for three different answers.
/// With nothing listening the dispute simply cannot be filed: it is the daemon
/// that writes `<state-dir>/disputes/<stage>/<n>/request.md` and moves the stage to
/// `NeedsAdjudication`, so there is no local fallback to take here the way
/// `loom stage block` has one. `Unreachable` is different — it says nothing
/// about the daemon, only that this process may not use unix sockets — so it
/// queues the dispute rather than concluding there is no daemon (see
/// `queue_dispute_request`).
fn dispute_via_socket(
    stage_id: String,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let req = build_request(
        &work_dir,
        &stage_id,
        criterion_index,
        &reason,
        &evidence_commit,
        &failure_output,
    );

    match try_send_request(&work_dir, &req)? {
        DaemonReach::Answered(response) => {
            handle_dispute_response(&stage_id, criterion_index, &reason, response)
        }
        DaemonReach::NotListening => bail!(
            "No daemon is listening on the state directory's orchestrator.sock, so the dispute \
             cannot be filed. The criterion stands until a daemon is running."
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

/// Relay a dispute request instead of filing it over the daemon socket.
#[allow(clippy::too_many_arguments)]
fn dispute_via_relay(
    stage_id: &str,
    criterion_index: usize,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::Dispute, Some(stage_id), cwd, uid)?;

    let request = StageRequest::Dispute {
        criterion_index,
        reason,
        evidence_commit,
        failure_output,
    };
    let payload = serde_json::to_value(&request).context("failed to serialize dispute request")?;
    context.emit(
        RequestKind::Dispute,
        payload,
        "stage dispute-criteria",
        false,
        sink,
    )?;
    Ok(())
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
                "The stage is now in NeedsAdjudication and this session's turn is over. When \
                 the verdict is applied, the daemon writes this session's handoff, retires it, \
                 and starts a fresh session against the amended criteria. Do not continue \
                 working the stage and do not run `loom stage complete` — this session may end \
                 now."
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
#[path = "dispute_criteria_tests.rs"]
mod tests;
