//! Decides whether a CLI invocation running inside a sandboxed session may
//! relay a request instead of writing the state directory directly, and
//! writes the ticket plus the `LOOM_RELAY_V1` line once it may.
//!
//! See `doc/plans/PLAN-loom-state-confinement.md` sections 4 and 6. Every
//! loom CLI writer is meant to call [`mode`] first; for [`RelayMode::Relay`],
//! [`RelayContext::check`] before touching anything and [`RelayContext::emit`]
//! (or [`RelayContext::emit_quiet`]) instead of writing the state directory.
//! Wiring individual commands to this contract is separate work — this
//! module only builds the shared piece.

mod cwd;
mod quota;
mod session_type;
mod sink;
mod stderr_text;
mod ticket_io;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests_check;
#[cfg(test)]
mod tests_emit;
#[cfg(test)]
mod tests_mode;

pub use sink::{RelaySink, StdSink};

use crate::models::session::SessionType;
use crate::relay::{
    new_request_id, sha256_hex, validate_session_dir, verdict, MatrixVerdict, RelayLine,
    RequestKind, Ticket, MAX_TICKET_BYTES,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A snapshot of the environment variables [`mode`] and [`RelayContext`]
/// decide from: `LOOM_SESSION_ID`, `LOOM_SCRATCH_DIR`, `LOOM_STAGE_ID`,
/// `LOOM_SESSION_TYPE` (kept as the raw string), `LOOM_WORKTREE_PATH`,
/// `LOOM_WORK_DIR`, and the `LOOM_HOOK_CONTEXT`/`LOOM_CONTROL_BROKER` flags.
/// Nothing in this module reads the process environment outside
/// [`EnvSnapshot::from_process_env`].
#[derive(Debug, Clone, Default)]
pub struct EnvSnapshot {
    pub session_id: Option<String>,
    pub scratch_dir: Option<PathBuf>,
    pub stage_id: Option<String>,
    pub session_type: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub work_dir: Option<PathBuf>,
    pub hook_context: bool,
    pub control_broker: bool,
}

impl EnvSnapshot {
    /// Read the real process environment.
    ///
    /// Under `cfg(test)` this returns an empty snapshot instead: the
    /// environment-derived path is never exercised by a test, so every
    /// existing command test keeps today's behavior, and a test that wants
    /// relay behavior builds an `EnvSnapshot` by hand.
    #[cfg(not(test))]
    pub fn from_process_env() -> Self {
        Self {
            session_id: std::env::var("LOOM_SESSION_ID").ok(),
            scratch_dir: std::env::var_os("LOOM_SCRATCH_DIR").map(PathBuf::from),
            stage_id: std::env::var("LOOM_STAGE_ID").ok(),
            session_type: std::env::var("LOOM_SESSION_TYPE").ok(),
            worktree_path: std::env::var_os("LOOM_WORKTREE_PATH").map(PathBuf::from),
            work_dir: std::env::var_os("LOOM_WORK_DIR").map(PathBuf::from),
            hook_context: std::env::var("LOOM_HOOK_CONTEXT").ok().as_deref() == Some("1"),
            control_broker: std::env::var("LOOM_CONTROL_BROKER").ok().as_deref() == Some("1"),
        }
    }

    #[cfg(test)]
    pub fn from_process_env() -> Self {
        Self::default()
    }
}

/// Which regime a CLI invocation runs under, decided from environment
/// presence only — never from a write error.
#[derive(Debug, Clone)]
pub enum RelayMode {
    /// Both `LOOM_SESSION_ID` and `LOOM_SCRATCH_DIR` are set, and neither a
    /// hook nor the control broker is running: CLI writes must go through a
    /// ticket.
    Relay(RelayContext),
    /// `LOOM_SESSION_ID` is set but `LOOM_SCRATCH_DIR` is not — a session
    /// spawned before the relay upgrade. Writes directly, unchanged.
    Legacy,
    /// No session identity, or a hook/control-broker process running outside
    /// the sandbox. Writes directly or via the socket, as today.
    Operator,
}

/// Decide [`RelayMode`] from `env`. A hook (`pre-compact.sh`, `session-end.sh`,
/// the ask-user hooks) or the control broker always writes directly, even
/// when it happens to inherit a scratch directory from its parent session.
pub fn mode(env: &EnvSnapshot) -> RelayMode {
    let Some(session_id) = env.session_id.clone() else {
        return RelayMode::Operator;
    };
    if env.hook_context || env.control_broker {
        return RelayMode::Operator;
    }
    let Some(scratch_dir) = env.scratch_dir.clone() else {
        return RelayMode::Legacy;
    };
    RelayMode::Relay(RelayContext {
        session_id,
        scratch_dir,
        stage_id: env.stage_id.clone(),
        session_type: env.session_type.clone(),
        worktree_path: env.worktree_path.clone(),
        work_dir: env.work_dir.clone(),
    })
}

/// Everything a Relay-mode CLI invocation needs to decide whether it may
/// relay a request, and to write one once it may.
#[derive(Debug, Clone)]
pub struct RelayContext {
    pub session_id: String,
    pub scratch_dir: PathBuf,
    pub stage_id: Option<String>,
    pub session_type: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub work_dir: Option<PathBuf>,
}

impl RelayContext {
    /// Every guard section 6 requires before a ticket may be written.
    /// Refuses with a clear, actionable error on the first failing guard;
    /// nothing is ever written by this call.
    pub fn check(
        &self,
        kind: RequestKind,
        stage_arg: Option<&str>,
        cwd: &Path,
        uid: u32,
    ) -> Result<()> {
        validate_session_dir(&self.scratch_dir, &self.session_id, uid)
            .context("relay scratch directory failed validation; refusing to relay")?;

        let session_type = self.parsed_session_type()?;
        cwd::require_within_checkout(self, session_type, cwd)?;
        self.require_matching_stage(stage_arg)?;

        if verdict(session_type, kind) == MatrixVerdict::Refuse {
            bail!(
                "a {session_type} session may not relay a '{kind}' request; refusing before any \
                 ticket is written"
            );
        }

        quota::require_headroom(&self.scratch_dir)
    }

    /// Write a ticket for `kind`/`payload`, print the `LOOM_RELAY_V1` line as
    /// `sink`'s last stdout line, and write the human PENDING RELAY notice
    /// (naming `what`, plus the end-turn reminder when `end_turn`) to its
    /// stderr first. Call only after [`Self::check`] has approved this
    /// request.
    pub fn emit(
        &self,
        kind: RequestKind,
        payload: Value,
        what: &str,
        end_turn: bool,
        sink: &mut dyn RelaySink,
    ) -> Result<RelayLine> {
        self.emit_ticket(kind, payload, sink, Some((what, end_turn)))
    }

    /// As [`Self::emit`], but writes nothing to `sink`'s stderr — for a
    /// request relayed on every call (`loom knowledge context`'s telemetry),
    /// where the five-line PENDING RELAY notice would make routine lookups
    /// noisy.
    pub fn emit_quiet(
        &self,
        kind: RequestKind,
        payload: Value,
        sink: &mut dyn RelaySink,
    ) -> Result<RelayLine> {
        self.emit_ticket(kind, payload, sink, None)
    }

    fn emit_ticket(
        &self,
        kind: RequestKind,
        payload: Value,
        sink: &mut dyn RelaySink,
        status: Option<(&str, bool)>,
    ) -> Result<RelayLine> {
        let ticket = Ticket {
            v: 1,
            id: new_request_id(),
            kind,
            created_at: Utc::now(),
            payload,
        };
        let bytes = ticket.encode();
        if bytes.len() > MAX_TICKET_BYTES {
            bail!(
                "ticket is {} bytes, over the {MAX_TICKET_BYTES}-byte relay limit; refusing to \
                 write it",
                bytes.len()
            );
        }

        ticket_io::write_ticket(&self.scratch_dir, &ticket.id, &bytes)?;

        if let Some((what, end_turn)) = status {
            let text = stderr_text::build(&ticket.id, what, end_turn);
            write!(sink.stderr(), "{text}").context("failed to write the relay status text")?;
            sink.stderr().flush().context("failed to flush stderr")?;
        }

        let line = RelayLine {
            kind,
            id: ticket.id,
            sha256: sha256_hex(&bytes),
            bytes: bytes.len() as u32,
        };
        writeln!(sink.stdout(), "{}", line.format()).context("failed to write the relay line")?;
        sink.stdout().flush().context("failed to flush stdout")?;
        Ok(line)
    }

    fn parsed_session_type(&self) -> Result<SessionType> {
        session_type::parse(self.session_type.as_deref().unwrap_or_default())
    }

    fn require_matching_stage(&self, stage_arg: Option<&str>) -> Result<()> {
        let Some(arg) = stage_arg else {
            return Ok(());
        };
        match self.stage_id.as_deref() {
            Some(stage_id) if stage_id == arg => Ok(()),
            Some(stage_id) => bail!(
                "a session relays only for its own stage '{stage_id}'; --stage '{arg}' does not \
                 match. Request was NOT relayed."
            ),
            None => bail!(
                "no active stage session (LOOM_STAGE_ID unset); --stage '{arg}' cannot be relayed"
            ),
        }
    }
}
