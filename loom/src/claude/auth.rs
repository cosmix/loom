//! Login probe for the claude CLI: `claude auth status --json`.
//!
//! Stage sessions run under the minimal environment of
//! [`crate::process::agent_session_environment_from`], not the daemon's, so the
//! probe that matters runs the CLI under that same environment
//! ([`stage_auth_status`]). The CLI's JSON carries identity (`email`, `orgId`,
//! `orgName`); nothing here reads, stores, or prints those fields.

use crate::process::{agent_session_environment_from, run_bounded, BoundedOutput};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Wall-clock bound on one `claude auth status` run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// What `claude auth status --json` says about the login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthProbe {
    /// The CLI is logged in; `method` is its `authMethod` (for example `claude.ai`).
    LoggedIn { method: String },
    /// The CLI reports it is not logged in.
    NotLoggedIn,
    /// The probe could not tell; the text is a fixed reason, never CLI output.
    Unknown(String),
}

/// Interpret the stdout of `claude auth status --json`.
///
/// Only `loggedIn` and `authMethod` are read. A logged-out status is
/// `NotLoggedIn` whatever the exit status (the CLI exits 1 for it); output that
/// does not parse, or lacks `loggedIn`, is `Unknown` whatever the exit status.
pub fn parse_auth_status(stdout: &str, exit_success: bool) -> AuthProbe {
    let value: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(value) => value,
        // serde_json's Display carries a line and column, never the input.
        Err(error) => return AuthProbe::Unknown(format!("unparseable auth status: {error}")),
    };
    match value.get("loggedIn").and_then(serde_json::Value::as_bool) {
        Some(true) => AuthProbe::LoggedIn {
            method: value
                .get("authMethod")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        },
        Some(false) => AuthProbe::NotLoggedIn,
        None => AuthProbe::Unknown(format!(
            "auth status has no loggedIn field (exit success: {exit_success})"
        )),
    }
}

/// Run `claude auth status --json`. `env` of `Some` replaces the environment
/// with exactly those pairs; `None` inherits the caller's.
fn run_auth_status(claude_path: &Path, env: Option<Vec<(OsString, OsString)>>) -> AuthProbe {
    let mut command = Command::new(claude_path);
    command
        .args(["auth", "status", "--json"])
        .stdin(Stdio::null());
    if let Some(env) = env {
        command.env_clear().envs(env);
    }
    match run_bounded(&mut command, PROBE_TIMEOUT) {
        Ok(BoundedOutput::Completed(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_auth_status(&stdout, output.status.success())
        }
        Ok(BoundedOutput::TimedOut) => {
            AuthProbe::Unknown(format!("timed out after {}s", PROBE_TIMEOUT.as_secs()))
        }
        Err(error) => {
            let kind = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<std::io::Error>())
                .map(|io| format!("{:?}", io.kind()));
            AuthProbe::Unknown(match kind {
                Some(kind) => format!("could not run claude: {kind}"),
                None => "could not run claude".to_string(),
            })
        }
    }
}

/// The login verdict under the environment stage sessions receive.
pub fn stage_auth_status(claude_path: &Path) -> AuthProbe {
    stage_auth_status_from(claude_path, std::env::vars_os())
}

/// [`stage_auth_status`] over an explicit environment source.
pub(crate) fn stage_auth_status_from<I, K, V>(claude_path: &Path, source: I) -> AuthProbe
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    run_auth_status(claude_path, Some(agent_session_environment_from(source)))
}

/// The login verdict under the operator's own environment, for telling a
/// logged-out CLI apart from one that only the stage environment cannot see.
pub fn operator_auth_status(claude_path: &Path) -> AuthProbe {
    run_auth_status(claude_path, None)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
