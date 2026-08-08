//! Claude Code Remote Control integration for the native backend.
//!
//! Remote Control lets the loom orchestrator drive Claude Code sessions
//! programmatically. It is gated behind a preflight check because the
//! `claude --remote-control` flag exits non-zero when its prerequisites are
//! not met (an unsupported claude version, or an auth setup that is not
//! claude.ai login based).
//!
//! Resolution model:
//!   * `RemoteControlConfig` (persisted in `.work/config.toml [remote_control]`)
//!     carries the operator-facing on/off switch (`mode = auto | off`).
//!   * `preflight()` combines a version probe with an auth-eligibility
//!     heuristic and yields a `RemoteControlStatus`.
//!   * `resolve()` is the mode/marker/preflight gate: it returns `false` when
//!     the mode is `off`, when a `.work/remote_control-unsupported` marker
//!     exists, or when the preflight is not satisfied. The marker lets the
//!     crash handler disable Remote Control mid-run after a fast-fail crash.
//!     Its only remaining caller is the crash handler's fast-fail check.
//!   * `resolve_invocation()` is the actual per-spawn decision point: it
//!     layers a memoized `--help` capability probe over `resolve()` to decide
//!     between `RemoteControlInvocation::Disabled`, `Bare` (older claude, no
//!     optional-name support), and `Named(session_name)`.

use crate::claude::find_claude_path;
use crate::fs::work_dir::read_remote_control_config;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Minimum claude version that supports the `--remote-control` flag.
const MIN_REMOTE_CONTROL_VERSION: (u64, u64, u64) = (2, 1, 51);

/// Filename (under the `.work` directory) of the marker that disables Remote
/// Control for the remainder of a run after a fast-fail crash.
const UNSUPPORTED_MARKER: &str = "remote_control-unsupported";

/// Environment variables whose presence indicates an auth setup that is NOT
/// claude.ai login based. Remote Control relies on claude.ai login, so any of
/// these disqualifies it. Only the variable *name* is ever surfaced — never
/// its value.
const DISQUALIFYING_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// Operator-facing Remote Control switch, persisted in `.work/config.toml`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteControlMode {
    /// Enable Remote Control whenever preflight passes (default).
    #[default]
    Auto,
    /// Never enable Remote Control, regardless of preflight.
    Off,
}

/// Persisted `[remote_control]` section of `.work/config.toml`.
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
/// [`MIN_REMOTE_CONTROL_VERSION`]. Exec failure or parse failure yields
/// `false` (fail closed).
pub fn claude_supports_remote_control(claude_path: &Path) -> bool {
    match probe_claude_version(claude_path) {
        Some(version) => version_supported(version),
        None => false,
    }
}

/// Heuristic check that the host's claude auth setup is eligible for Remote
/// Control (which requires claude.ai login).
///
/// Returns `Err` with a non-secret reason — naming only the offending
/// environment variable, never its value — when:
///   * a disqualifying auth env var is set, or
///   * no disqualifying var is set but neither `~/.claude/.credentials.json`
///     nor (on macOS) a "Claude Code-credentials" Keychain entry is found (no
///     claude.ai login found).
///
/// Returns `Ok(())` when either the credentials file or the macOS Keychain
/// entry is present and no disqualifying env var is set. On macOS, Claude
/// Code stores credentials in the Keychain instead of the file, so both
/// locations must be checked.
pub fn remote_control_eligible() -> Result<(), String> {
    for var in DISQUALIFYING_ENV_VARS {
        if std::env::var_os(var).is_some() {
            return Err(format!(
                "{var} is set (Remote Control requires claude.ai login auth)"
            ));
        }
    }

    let credentials_present = dirs::home_dir()
        .map(|h| h.join(".claude").join(".credentials.json").exists())
        .unwrap_or(false);

    if credentials_present || macos_keychain_has_credentials() {
        Ok(())
    } else {
        Err(
            "claude.ai login not found (no ~/.claude/.credentials.json and no macOS Keychain entry)"
                .to_string(),
        )
    }
}

/// Pure builder for the macOS Keychain lookup argv.
///
/// Shared by [`macos_keychain_has_credentials`] (so the actual command can
/// never drift from what is tested) and asserted directly by
/// `keychain_probe_argv_is_exact` below. Deliberately excludes `-w`, which
/// would print the stored secret to stdout — this lookup only ever checks
/// for the entry's existence.
pub(crate) fn keychain_probe_argv() -> (&'static str, [&'static str; 3]) {
    (
        "security",
        ["find-generic-password", "-s", "Claude Code-credentials"],
    )
}

/// Whether a "Claude Code-credentials" entry exists in the macOS Keychain.
///
/// Never surfaces the secret value: `security find-generic-password` (no
/// `-w`) only communicates success/failure via its exit status, and both
/// stdout and stderr are discarded here.
///
/// Always `false` off macOS. The gate is a runtime `cfg!` rather than a
/// `#[cfg]` pair so the body — and therefore [`keychain_probe_argv`] — is
/// compiled on every platform; under `#[cfg]` the probe builder would have no
/// non-test caller on Linux and trip `dead_code` under `-D warnings`.
fn macos_keychain_has_credentials() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let (program, args) = keychain_probe_argv();
    Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Combine the version probe and the auth-eligibility heuristic into a single
/// [`RemoteControlStatus`].
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

    if let Err(reason) = remote_control_eligible() {
        return RemoteControlStatus::Disabled { reason };
    }

    RemoteControlStatus::Enabled
}

/// Path to the `.work/remote_control-unsupported` marker file.
fn unsupported_marker_path(work_dir: &Path) -> std::path::PathBuf {
    work_dir.join(UNSUPPORTED_MARKER)
}

/// Whether the mid-run "Remote Control unsupported" marker exists.
pub fn unsupported_marker_exists(work_dir: &Path) -> bool {
    unsupported_marker_path(work_dir).exists()
}

/// Write the `.work/remote_control-unsupported` marker.
///
/// Best-effort: write errors are returned to the caller, which typically
/// ignores them (the marker is an optimization, not a correctness gate).
pub fn write_unsupported_marker(work_dir: &Path) -> std::io::Result<()> {
    std::fs::write(
        unsupported_marker_path(work_dir),
        "Remote Control disabled after a fast-fail session crash.\n",
    )
}

/// Memoized version-probe result, keyed by nothing — `claude --version` is
/// invariant for the lifetime of a process. `None` means "not yet probed".
fn cached_preflight_enabled(claude_path: &Path) -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| preflight(claude_path).is_enabled())
}

/// Per-spawn gate: whether `--remote-control` should be appended for a session
/// spawned against `work_dir`.
///
/// Returns `false` when:
///   * the persisted `[remote_control]` mode is `off`,
///   * the `.work/remote_control-unsupported` marker exists, or
///   * the (memoized) preflight is not satisfied.
///
/// Config and marker are re-read every call (both cheap) so an operator
/// toggling the mode or a mid-run marker write takes effect immediately. The
/// `claude --version` subprocess behind the preflight runs at most once per
/// process.
///
/// All errors are swallowed (treated as "disabled") so a spawn site can call
/// this unconditionally.
pub fn resolve(work_dir: &Path) -> bool {
    let mode = read_remote_control_config(work_dir)
        .map(|c| c.mode)
        .unwrap_or_default();
    if mode == RemoteControlMode::Off {
        return false;
    }

    if unsupported_marker_exists(work_dir) {
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
/// The config and unsupported-marker are re-read on every call (via
/// [`resolve`], cheap); the `--help` capability probe runs at most once per
/// process (memoized in [`cached_named_arg_supported`]). [`resolve`] itself is
/// UNCHANGED — the crash handler keeps calling it directly and this function
/// does not alter its `bool` contract.
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
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn default_config_mode_is_auto() {
        let config = RemoteControlConfig::default();
        assert_eq!(config.mode, RemoteControlMode::Auto);
    }

    #[test]
    fn default_mode_is_auto() {
        assert_eq!(RemoteControlMode::default(), RemoteControlMode::Auto);
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = RemoteControlConfig {
            mode: RemoteControlMode::Off,
        };
        let rendered = toml::to_string(&config).unwrap();
        assert!(rendered.contains("off"), "rendered: {rendered}");
        let parsed: RemoteControlConfig = toml::from_str(&rendered).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn missing_mode_defaults_to_auto() {
        let parsed: RemoteControlConfig = toml::from_str("").unwrap();
        assert_eq!(parsed.mode, RemoteControlMode::Auto);
    }

    #[test]
    fn parse_version_handles_plain_and_noisy() {
        assert_eq!(parse_version("2.1.51"), Some((2, 1, 51)));
        assert_eq!(parse_version("2.1.51 (Claude Code)"), Some((2, 1, 51)));
        assert_eq!(parse_version("v10.20.30"), Some((10, 20, 30)));
        assert_eq!(parse_version("not a version"), None);
        assert_eq!(parse_version("2.1"), None);
    }

    #[test]
    fn version_supported_covers_boundaries() {
        // Exact minimum supported version.
        assert!(version_supported(MIN_REMOTE_CONTROL_VERSION));
        assert!(version_supported((2, 1, 51)));
        // One patch below the minimum — unsupported.
        assert!(!version_supported((2, 1, 50)));
        // Newer patch / minor / major — all supported.
        assert!(version_supported((2, 1, 52)));
        assert!(version_supported((2, 2, 0)));
        assert!(version_supported((3, 0, 0)));
        // Older minor / major — unsupported.
        assert!(!version_supported((2, 0, 99)));
        assert!(!version_supported((1, 9, 9)));
    }

    #[test]
    fn status_is_enabled_reports_correctly() {
        assert!(RemoteControlStatus::Enabled.is_enabled());
        assert!(!RemoteControlStatus::Disabled {
            reason: "x".to_string()
        }
        .is_enabled());
    }

    #[test]
    fn supports_remote_control_false_for_missing_binary() {
        // A path that does not exist must fail closed.
        assert!(!claude_supports_remote_control(Path::new(
            "/nonexistent/claude-binary-xyz"
        )));
    }

    #[test]
    #[serial]
    fn eligible_rejects_disqualifying_env_var() {
        // Save and restore every disqualifying var so the test is hermetic.
        let saved: Vec<(&str, Option<std::ffi::OsString>)> = DISQUALIFYING_ENV_VARS
            .iter()
            .map(|v| (*v, std::env::var_os(v)))
            .collect();
        for (var, _) in &saved {
            unsafe { std::env::remove_var(var) };
        }

        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "super-secret-value") };
        let result = remote_control_eligible();

        // Restore environment before asserting.
        for (var, value) in &saved {
            match value {
                Some(v) => unsafe { std::env::set_var(var, v) },
                None => unsafe { std::env::remove_var(var) },
            }
        }

        let err = result.expect_err("disqualifying env var must produce Err");
        assert!(
            err.contains("ANTHROPIC_API_KEY"),
            "reason must name the var: {err}"
        );
        assert!(
            !err.contains("super-secret-value"),
            "reason must NEVER contain the var value: {err}"
        );
    }

    #[test]
    #[serial]
    fn resolve_false_when_mode_off() {
        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path();
        crate::fs::work_dir::write_remote_control_config(
            work_dir,
            &RemoteControlConfig {
                mode: RemoteControlMode::Off,
            },
        )
        .unwrap();
        assert!(!resolve(work_dir));
    }

    #[test]
    fn keychain_probe_argv_is_exact() {
        let (program, args) = keychain_probe_argv();
        assert_eq!(program, "security");
        assert_eq!(args[0], "find-generic-password");
        assert_eq!(args[1], "-s");
        assert_eq!(args[2], "Claude Code-credentials");
        assert!(
            !args.contains(&"-w"),
            "must never pass -w: it prints the secret to stdout"
        );
    }

    #[test]
    fn help_indicates_named_arg_detects_optional_name() {
        let help = "Usage: claude [options] [prompt]\n\
                    \x20 --permission-mode <mode>  Permission mode\n\
                    \x20 --remote-control [name]   Enable remote control\n";
        assert!(help_indicates_named_arg(help));
    }

    #[test]
    fn help_indicates_named_arg_false_without_optional_name() {
        // Older claude: the flag exists but takes no argument.
        let help = "  --remote-control        Enable remote control\n";
        assert!(!help_indicates_named_arg(help));
    }

    #[test]
    fn probe_named_arg_support_false_for_missing_binary() {
        // A path that does not exist must fail closed (bare flag).
        assert!(!probe_named_arg_support(Path::new(
            "/nonexistent/claude-binary-xyz"
        )));
    }

    #[test]
    #[serial]
    fn resolve_invocation_disabled_when_mode_off() {
        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path();
        crate::fs::work_dir::write_remote_control_config(
            work_dir,
            &RemoteControlConfig {
                mode: RemoteControlMode::Off,
            },
        )
        .unwrap();
        assert_eq!(
            resolve_invocation(work_dir, "anything"),
            RemoteControlInvocation::Disabled
        );
    }

    #[test]
    #[serial]
    fn resolve_false_when_unsupported_marker_present() {
        let temp = tempfile::TempDir::new().unwrap();
        let work_dir = temp.path();
        // mode auto (default, no config written)
        write_unsupported_marker(work_dir).unwrap();
        assert!(unsupported_marker_exists(work_dir));
        assert!(!resolve(work_dir));
    }
}
