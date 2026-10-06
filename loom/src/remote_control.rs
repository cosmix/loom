//! Claude Code Remote Control integration for the native backend.
//!
//! Remote Control lets the loom orchestrator drive Claude Code sessions
//! programmatically, gated behind a preflight check: `claude
//! --remote-control` exits non-zero unless the claude version and login
//! (claude.ai) both qualify.
//!
//! Resolution model:
//!   * `RemoteControlConfig` (persisted in `.loom/work/config.toml [remote_control]`)
//!     carries the operator-facing on/off switch (`mode = auto | off`).
//!   * `preflight()` combines a version probe with the verdict of
//!     `claude auth status --json` run under the environment stage sessions
//!     receive (`eligibility_from`) and yields a `RemoteControlStatus`. There
//!     is no credentials-file, Keychain or environment-variable heuristic.
//!   * `resolve()` is the mode/preflight gate: `false` when the mode is
//!     `off`, the preflight fails, or [`disable_for_this_process`] has
//!     latched it off (in-memory only, set by the crash handler).
//!   * `resolve_invocation()` is the actual per-spawn decision point: it
//!     layers a memoized `--help` capability probe over `resolve()` to decide
//!     between `RemoteControlInvocation::Disabled`, `Bare` (older claude, no
//!     optional-name support), and `Named(session_name)`.

use crate::claude::auth::{stage_auth_status, AuthProbe};
use crate::claude::find_claude_path;
use crate::fs::work_dir::read_remote_control_config;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Minimum claude version that supports the `--remote-control` flag.
const MIN_REMOTE_CONTROL_VERSION: (u64, u64, u64) = (2, 1, 51);

/// Operator-facing Remote Control switch, persisted in `.loom/work/config.toml`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteControlMode {
    /// Enable Remote Control whenever preflight passes (default).
    #[default]
    Auto,
    /// Never enable Remote Control, regardless of preflight.
    Off,
}

/// Persisted `[remote_control]` section of `.loom/work/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteControlConfig {
    /// The operator-facing on/off switch. Defaults to `auto`.
    #[serde(default)]
    pub mode: RemoteControlMode,
}

/// Result of a Remote Control preflight check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteControlStatus {
    /// Remote Control prerequisites are satisfied.
    Enabled,
    /// Remote Control is unavailable; `reason` is a non-secret explanation.
    Disabled { reason: String },
}

impl RemoteControlStatus {
    /// Whether Remote Control is enabled.
    pub fn is_enabled(&self) -> bool {
        matches!(self, RemoteControlStatus::Enabled)
    }
}

/// The concrete `--remote-control` invocation to emit for a spawn, decided by
/// [`resolve_invocation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteControlInvocation {
    /// Omit the flag entirely.
    Disabled,
    /// Pass `--remote-control` with no argument (older claude versions that
    /// accept the flag but not the optional name argument).
    Bare,
    /// Pass `--remote-control <name>`.
    Named(String),
}

/// Parse a semver triple (`major.minor.patch`) out of arbitrary text.
///
/// Tolerates surrounding noise (e.g. `"2.1.51 (Claude Code)"`). Returns `None`
/// when no `X.Y.Z` token is found.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    for token in text.split_whitespace() {
        let cleaned: &str = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if let Some(version) = parse_version_token(cleaned) {
            return Some(version);
        }
    }
    None
}

/// Parse a single `X.Y.Z` token. Returns `None` if any component is missing
/// or non-numeric, so the caller can try the next whitespace-separated token.
fn parse_version_token(token: &str) -> Option<(u64, u64, u64)> {
    let mut parts = token.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Whether a parsed version triple satisfies [`MIN_REMOTE_CONTROL_VERSION`].
///
/// The single source of truth for the version gate, shared by
/// [`claude_supports_remote_control`] and [`preflight`].
fn version_supported(version: (u64, u64, u64)) -> bool {
    version >= MIN_REMOTE_CONTROL_VERSION
}

/// Run `<claude_path> --version` and return the parsed version string.
///
/// Returns `None` on exec failure or unparseable output.
fn probe_claude_version(claude_path: &Path) -> Option<(u64, u64, u64)> {
    let output = Command::new(claude_path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_version(&stdout)
}

/// Whether `claude --help` output documents the optional `[name]` argument to
/// `--remote-control`.
fn help_indicates_named_arg(help_text: &str) -> bool {
    help_text.contains("--remote-control [name]")
}

/// Whether `<claude_path> --help` documents the optional `[name]` argument to
/// `--remote-control`. Exec failure or non-zero exit fails closed (`false`,
/// i.e. fall back to the bare flag).
fn probe_named_arg_support(claude_path: &Path) -> bool {
    let Ok(output) = Command::new(claude_path).arg("--help").output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    help_indicates_named_arg(&stdout)
}

/// Whether the claude binary at `claude_path` supports `--remote-control`.
///
/// Runs `claude --version` and compares against
/// `MIN_REMOTE_CONTROL_VERSION`. Exec failure or parse failure yields
/// `false` (fail closed).
pub fn claude_supports_remote_control(claude_path: &Path) -> bool {
    match probe_claude_version(claude_path) {
        Some(version) => version_supported(version),
        None => false,
    }
}

/// Memoized login probe under the stage environment. `claude auth status`
/// is invariant for the process's purposes, and `loom run` seeds this through
/// `auth_preflight::require_stage_login` so the probe runs once per process.
pub(crate) fn cached_stage_auth(claude_path: &Path) -> &'static AuthProbe {
    static CACHE: OnceLock<AuthProbe> = OnceLock::new();
    CACHE.get_or_init(|| stage_auth_status(claude_path))
}

/// Longest auth method name a reason repeats.
const MAX_METHOD_CHARS: usize = 32;

/// `method` reduced to `[A-Za-z0-9._-]`, at most [`MAX_METHOD_CHARS`] characters.
/// It is the CLI's `authMethod` string, printed to stderr in a reason: a newline
/// or an escape sequence in it must not reach the log.
fn method_label(method: &str) -> String {
    method
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(MAX_METHOD_CHARS)
        .collect()
}

/// Whether a login verdict is eligible for Remote Control, which requires a
/// claude.ai login. Reasons name only the auth method (see [`method_label`]) or
/// a fixed probe reason, never identity.
fn eligibility_from(probe: &AuthProbe) -> Result<()> {
    match probe {
        AuthProbe::LoggedIn { method } if method == "claude.ai" => Ok(()),
        AuthProbe::LoggedIn { method } => bail!(
            "claude is logged in with {}, but Remote Control requires claude.ai login",
            method_label(method)
        ),
        AuthProbe::NotLoggedIn => bail!("claude is not logged in under the stage environment"),
        AuthProbe::Unknown(reason) => bail!("could not determine the claude login ({reason})"),
    }
}

/// Check that the claude login under the stage environment is eligible for
/// Remote Control (claude.ai login).
pub fn remote_control_eligible(claude_path: &Path) -> Result<()> {
    eligibility_from(cached_stage_auth(claude_path))
}

/// Combine the version probe and the stage-environment login verdict into a
/// single [`RemoteControlStatus`].
pub fn preflight(claude_path: &Path) -> RemoteControlStatus {
    let version = probe_claude_version(claude_path);
    match version {
        Some(v) if version_supported(v) => {}
        Some(v) => {
            return RemoteControlStatus::Disabled {
                reason: format!(
                    "claude {}.{}.{} < {}.{}.{}",
                    v.0,
                    v.1,
                    v.2,
                    MIN_REMOTE_CONTROL_VERSION.0,
                    MIN_REMOTE_CONTROL_VERSION.1,
                    MIN_REMOTE_CONTROL_VERSION.2,
                ),
            };
        }
        None => {
            return RemoteControlStatus::Disabled {
                reason: format!(
                    "could not determine claude version (need >= {}.{}.{})",
                    MIN_REMOTE_CONTROL_VERSION.0,
                    MIN_REMOTE_CONTROL_VERSION.1,
                    MIN_REMOTE_CONTROL_VERSION.2,
                ),
            };
        }
    }

    if let Err(reason) = remote_control_eligible(claude_path) {
        return RemoteControlStatus::Disabled {
            reason: reason.to_string(),
        };
    }

    RemoteControlStatus::Enabled
}

/// Memoized version-probe result, keyed by nothing — `claude --version` is
/// invariant for the lifetime of a process. `None` means "not yet probed".
fn cached_preflight_enabled(claude_path: &Path) -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| preflight(claude_path).is_enabled())
}

/// Process-lifetime "Remote Control unavailable" latch, set by
/// [`disable_for_this_process`]. Never persisted — a daemon restart starts
/// clear and probes again.
static DISABLED_FOR_PROCESS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Latch Remote Control off for the rest of this process and log the reason
/// to stderr once (the daemon's stderr is `orchestrator.log`). Idempotent —
/// a later call keeps the first reason — and never touches disk, so a CLI
/// invocation is unaffected and a daemon restart tries again.
pub fn disable_for_this_process(reason: &str) {
    let mut latch = DISABLED_FOR_PROCESS.lock().unwrap();
    if latch.is_some() {
        return;
    }
    *latch = Some(reason.to_string());
    eprintln!(
        "Remote Control unavailable ({reason}); continuing without it for the rest of this \
         daemon run. Set [remote_control] mode = \"off\" to stop trying it."
    );
}

/// Whether [`disable_for_this_process`] has latched for this process.
pub(crate) fn disabled_for_process() -> bool {
    DISABLED_FOR_PROCESS.lock().unwrap().is_some()
}

/// Test-only reset of the latch. Callers must be `#[serial]`.
#[cfg(test)]
pub(crate) fn reset_disabled_for_process() {
    *DISABLED_FOR_PROCESS.lock().unwrap() = None;
}

/// Per-spawn gate: whether `--remote-control` should be appended for a
/// session spawned against `work_dir`. Returns `false` when
/// [`disable_for_this_process`] has latched (never persisted; a daemon
/// restart clears it), the persisted `[remote_control]` mode is `off`, or
/// the (memoized) preflight is not satisfied.
///
/// Config is re-read every call (cheap); the `claude --version` subprocess
/// behind the preflight runs at most once per process. All errors are
/// swallowed (treated as "disabled") so a spawn site can call this
/// unconditionally.
pub fn resolve(work_dir: &Path) -> bool {
    if disabled_for_process() {
        return false;
    }

    let mode = read_remote_control_config(work_dir)
        .map(|c| c.mode)
        .unwrap_or_default();
    if mode == RemoteControlMode::Off {
        return false;
    }

    match find_claude_path() {
        Ok(path) => cached_preflight_enabled(&path),
        Err(_) => false,
    }
}

/// Memoized named-argument capability probe, keyed by nothing — `claude
/// --help` output is invariant for the daemon's lifetime. Separate from the
/// version-preflight cache in [`cached_preflight_enabled`].
fn cached_named_arg_supported(claude_path: &Path) -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| probe_named_arg_support(claude_path))
}

/// Resolve the concrete `--remote-control` invocation for a spawn against
/// `work_dir`, naming the session `session_name` when the installed claude
/// supports the optional name argument.
///
/// The config is re-read on every call (via [`resolve`]); the `--help`
/// capability probe runs at most once per process (memoized in
/// `cached_named_arg_supported`).
pub fn resolve_invocation(work_dir: &Path, session_name: &str) -> RemoteControlInvocation {
    if !resolve(work_dir) {
        return RemoteControlInvocation::Disabled;
    }

    match find_claude_path() {
        Err(_) => RemoteControlInvocation::Disabled,
        Ok(path) => {
            if cached_named_arg_supported(&path) {
                RemoteControlInvocation::Named(session_name.to_string())
            } else {
                RemoteControlInvocation::Bare
            }
        }
    }
}

/// Run the Remote Control preflight once at orchestrator startup and print an
/// advisory warning to stderr if it is disabled.
///
/// This is purely advisory — it never aborts startup and never returns an
/// error. When the persisted mode is `off`, the probe is skipped entirely.
pub fn run_startup_preflight(claude_path: &Path, work_dir: &Path) {
    let mode = read_remote_control_config(work_dir)
        .map(|c| c.mode)
        .unwrap_or_default();
    if mode == RemoteControlMode::Off {
        // Operator explicitly disabled Remote Control; stay quiet.
        return;
    }

    match preflight(claude_path) {
        RemoteControlStatus::Enabled => {}
        RemoteControlStatus::Disabled { reason } => {
            eprintln!("\u{26a0} Remote Control disabled: {reason}");
        }
    }
}

#[cfg(test)]
#[path = "remote_control_tests.rs"]
mod tests;
