//! The transport every `loom stage dispute-*` command shares.
//!
//! None of them mutates stage state. In relay mode the dispute becomes a relay
//! ticket; otherwise it goes to the daemon's Unix socket as a structured RPC,
//! and a caller that may not use the socket queues it on the worktree spool.
//! The daemon writes `<state-dir>/disputes/<stage>/<n>/request.md`,
//! transitions the stage to `NeedsAdjudication`, and returns an allocated id.
//!
//! See `loom/src/daemon/server/dispute.rs` and `dispute_kinds.rs` for the
//! server-side handlers and `loom/src/models/dispute.rs` for the on-disk schema.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::daemon::{
    current_session_id, try_send_request, user_credential, DaemonReach, Request, Response,
};
use crate::fs::stage_request::{append_to_spool, spool_path, spool_target_from_cwd, StageRequest};
use crate::models::dispute::DisputeKind;
use crate::relay::emit::{RelayContext, RelayMode, RelaySink};
use crate::relay::RequestKind;

/// One dispute on its way to the daemon. A criterion dispute travels as
/// `DisputeCriteria`, every other kind as `FileDispute`.
pub(super) struct Dispute {
    stage_id: String,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    /// Criterion disputes only: the captured failure output.
    failure_output: Option<String>,
}

impl Dispute {
    /// `loom stage dispute-criteria`.
    pub(super) fn criterion(
        stage_id: String,
        criterion_index: usize,
        reason: String,
        evidence_commit: Option<String>,
        failure_output: Option<String>,
    ) -> Self {
        Self {
            stage_id,
            kind: DisputeKind::Criterion { criterion_index },
            reason,
            evidence_commit,
            failure_output,
        }
    }

    /// `loom stage dispute-findings`, `dispute-contract` or `dispute-integrity`.
    pub(super) fn of_kind(
        stage_id: String,
        kind: DisputeKind,
        reason: String,
        evidence_commit: Option<String>,
    ) -> Self {
        Self {
            stage_id,
            kind,
            reason,
            evidence_commit,
            failure_output: None,
        }
    }

    /// The spool line and relay payload: the RPC without its stage and session.
    fn queued(&self) -> StageRequest {
        let reason = self.reason.clone();
        let evidence_commit = self.evidence_commit.clone();
        match &self.kind {
            DisputeKind::Criterion { criterion_index } => StageRequest::Dispute {
                criterion_index: *criterion_index,
                reason,
                evidence_commit,
                failure_output: self.failure_output.clone(),
            },
            kind => StageRequest::FileDispute {
                kind: kind.clone(),
                reason,
                evidence_commit,
            },
        }
    }

    /// The RPC the daemon expects.
    ///
    /// A missing or unreadable token is the NORMAL case here, not an error: the
    /// agent that needs this command most is the one the sandbox denies the
    /// read to (S-1). It names the session it is running inside instead, and
    /// the daemon authorizes it by the connection.
    fn rpc(&self, work_dir: &Path) -> Request {
        let auth_token = user_credential(work_dir);
        let stage_id = self.stage_id.clone();
        let session_id = current_session_id();
        let reason = self.reason.clone();
        let evidence_commit = self.evidence_commit.clone();
        match &self.kind {
            DisputeKind::Criterion { criterion_index } => Request::DisputeCriteria {
                auth_token,
                stage_id,
                session_id,
                criterion_index: *criterion_index,
                reason,
                evidence_commit,
                failure_output: self.failure_output.clone(),
            },
            kind => Request::FileDispute {
                auth_token,
                stage_id,
                session_id,
                kind: kind.clone(),
                reason,
                evidence_commit,
            },
        }
    }

    fn wording(&self) -> Wording {
        let (subject, stands, command) = match &self.kind {
            DisputeKind::Criterion { criterion_index } => (
                format!("criterion {criterion_index}"),
                "The criterion stands",
                "stage dispute-criteria",
            ),
            DisputeKind::Findings { finding_ids, .. } => (
                format!("findings {}", finding_ids.join(", ")),
                "The findings stand",
                "stage dispute-findings",
            ),
            DisputeKind::Contract { contract_id } => (
                format!("contract {contract_id}"),
                "The contract stands",
                "stage dispute-contract",
            ),
            DisputeKind::Integrity { event_ids, .. } => (
                format!("integrity events {}", event_ids.join(", ")),
                "The integrity events stand",
                "stage dispute-integrity",
            ),
        };
        let (successor, rpc, relay_kind) = match self.kind {
            DisputeKind::Criterion { .. } => (
                "against the amended criteria",
                "DisputeCriteria",
                RequestKind::Dispute,
            ),
            _ => (
                "with the verdict applied",
                "FileDispute",
                RequestKind::FileDispute,
            ),
        };
        Wording {
            subject,
            stands,
            successor,
            command,
            rpc,
            relay_kind,
        }
    }
}

/// How the commands name a dispute, and the relay kind it travels as.
struct Wording {
    /// What the confirmation says was disputed: `criterion 2`.
    subject: String,
    /// The refusal when no daemon listens ends `<stands> until a daemon is running.`
    stands: &'static str,
    /// What the fresh session starts against once the verdict is applied.
    successor: &'static str,
    /// The command a relay notice names.
    command: &'static str,
    /// The RPC an unexpected answer is reported against.
    rpc: &'static str,
    relay_kind: RequestKind,
}

/// In Relay mode the dispute goes to the relay hook; otherwise the daemon
/// socket (see [`send_via_socket`]).
pub(super) fn send(
    dispute: Dispute,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        return send_via_relay(&dispute, &context, cwd, sink);
    }
    send_via_socket(&dispute)
}

/// The three ways the daemon can be reached call for three different answers.
/// With nothing listening the dispute simply cannot be filed: it is the daemon
/// that writes `<state-dir>/disputes/<stage>/<n>/request.md` and moves the stage to
/// `NeedsAdjudication`, so there is no local fallback to take here the way
/// `loom stage block` has one. `Unreachable` is different — it says nothing
/// about the daemon, only that this process may not use unix sockets — so it
/// queues the dispute rather than concluding there is no daemon (see
/// [`queue_dispute`]).
fn send_via_socket(dispute: &Dispute) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let wording = dispute.wording();
    match try_send_request(&work_dir, &dispute.rpc(&work_dir))? {
        DaemonReach::Answered(response) => handle_response(dispute, &wording, response),
        DaemonReach::NotListening => bail!(
            "No daemon is listening on the state directory's orchestrator.sock, so the dispute \
             cannot be filed. {} until a daemon is running.",
            wording.stands
        ),
        DaemonReach::Unreachable => queue_dispute(&dispute.stage_id, &dispute.queued()),
    }
}

/// Relay a dispute instead of filing it over the daemon socket.
fn send_via_relay(
    dispute: &Dispute,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let wording = dispute.wording();
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(wording.relay_kind, Some(&dispute.stage_id), cwd, uid)?;

    let payload =
        serde_json::to_value(dispute.queued()).context("failed to serialize dispute request")?;
    context.emit(wording.relay_kind, payload, wording.command, false, sink)?;
    Ok(())
}

/// Queue a dispute for the daemon to file, for the caller that cannot reach it.
///
/// Queueing does not weaken the authorization the RPC path establishes: the
/// daemon still runs the same handler, still enforces the id and
/// dispute-budget checks, and still attributes the dispute to the worktree it
/// drained it from rather than to anything the request claims about itself.
fn queue_dispute(stage_id: &str, request: &StageRequest) -> Result<()> {
    let worktree_root = spool_target_from_cwd()?;
    append_to_spool(&worktree_root, request)?;

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

/// Interpret the daemon's answer. A live daemon's refusal is authoritative and
/// reported verbatim — there is no local fallback to defer to (see the
/// `NotListening` arm in [`send_via_socket`]).
fn handle_response(dispute: &Dispute, wording: &Wording, response: Response) -> Result<()> {
    let stage_id = &dispute.stage_id;
    match response {
        Response::DisputeCreated { id } => {
            println!(
                "Filed dispute #{id} for stage '{stage_id}' ({}).",
                wording.subject
            );
            println!("Reason: {}", dispute.reason);
            println!();
            println!(
                "The stage is now in NeedsAdjudication and this session's turn is over. When \
                 the verdict is applied, the daemon writes this session's handoff, retires it, \
                 and starts a fresh session {}. Do not continue working the stage and do not \
                 run `loom stage complete` — this session may end now.",
                wording.successor
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
        other => bail!("Unexpected daemon response to {}: {other:?}", wording.rpc),
    }
}
