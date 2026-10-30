//! The seam between the bridge and whatever runs a session (launch, input, teardown,
//! liveness).
//!
//! Production uses `tmux_host::TmuxHost`; the bridge's tests use a fake. The bridge calls every
//! host method from its driver thread (or `abort`'s caller), never while it holds its lock.

use std::fmt;
use std::path::PathBuf;

use super::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::runtime::RuntimePaths;
use crate::process::{IdentityStatus, ProcessIdentity};

/// Everything a host needs to launch one planning session.
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub id: String,
    pub kind: SessionKind,
    pub dir: SessionDir,
    /// `loom-plan-<12 hex digits of the id>`.
    pub socket: String,
    /// `plan-<kind label>`; also the target of `deliver` and `interrupt`.
    pub tmux_session: String,
    /// The session's guidance with an empty `opening_message`: the bridge pastes the opening
    /// after the first `SessionStart`, so it never reaches the launch line (px-security-04).
    pub guidance: LaunchGuidance,
    pub extra_env: Vec<(String, String)>,
    /// The opening as the launch line's last word; only a validated command opening
    /// (`LaunchGuidance::opening_on_argv`), otherwise `None`.
    pub opening_argv: Option<String>,
    pub project_root: PathBuf,
    pub runtime: RuntimePaths,
    pub loom_bin: PathBuf,
    pub claude_bin: PathBuf,
    pub home: PathBuf,
}

/// The process a successful launch started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawned {
    pub identity: ProcessIdentity,
}

/// Runs planning sessions for the bridge.
pub trait SessionHost: Send + Sync {
    /// Start the session; an error leaves nothing running, except a [`TeardownFailed`], whose
    /// server may still run.
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned>;
    /// Paste `text` into the session's pane and submit it.
    fn deliver(&self, socket: &str, target: &str, text: &str) -> anyhow::Result<()>;
    /// Press Enter once in the session's pane: submits a pasted opening its first Enter missed.
    /// The default presses nothing, for the test hosts that keep no input box.
    fn press_enter(&self, _socket: &str, _target: &str) -> anyhow::Result<()> {
        Ok(())
    }
    /// Press Escape once in the session's pane.
    fn interrupt(&self, socket: &str, target: &str) -> anyhow::Result<()>;
    /// End the session's tmux server and remove its socket.
    fn teardown(&self, socket: &str) -> anyhow::Result<()>;
    /// Whether the session's process is still the one that was launched.
    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus;
}

/// A spawn that failed once its tmux server may have started, and whose teardown failed too:
/// the server may still run. The bridge keeps the session's directory and provisional record,
/// the only handle to that server, and retries the teardown; a later recovery reaps it.
#[derive(Debug)]
pub struct TeardownFailed {
    /// Why the spawn failed.
    pub error: anyhow::Error,
    /// Why the teardown after it failed.
    pub teardown: anyhow::Error,
}

impl fmt::Display for TeardownFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (error, teardown) = (&self.error, &self.teardown);
        write!(f, "{error:#}; its teardown also failed: {teardown:#}")
    }
}

impl std::error::Error for TeardownFailed {}

/// `loom-plan-<first 12 hex digits of the id>`.
pub(super) fn socket_name(id: &str) -> String {
    let hex: String = id.chars().filter(|c| *c != '-').take(12).collect();
    format!("loom-plan-{hex}")
}
