//! Blocking Linux/WSL sandbox-prerequisite preflight for `loom run`.
//!
//! The generated stage settings set `sandbox.failIfUnavailable: true` whenever
//! the plan sandbox is enabled (`sandbox::settings::policy`), so a host whose
//! Claude Code sandbox cannot initialize makes EVERY session exit at startup
//! with "sandbox required but unavailable" — before a tool runs, before the
//! crash report has anything to show. Refusing here, with the real missing
//! dependency named, is the same posture as `checks::require_jq`.

use anyhow::{bail, Result};
use std::path::Path;

/// Hard requirement — aborts startup when the persisted plan sandbox is
/// enabled and this Linux/WSL host cannot satisfy Claude Code's sandbox.
/// A no-op on any other OS (macOS's Seatbelt sandbox is built in) and when
/// the plan sandbox is disabled.
///
/// Reads the persisted `[plan_sandbox]` snapshot in `work_dir` — the same
/// source `plan/graph/loader.rs` uses whenever stage files already exist.
/// Only the plan-file fallback path (no stage files at all) re-parses the
/// plan directly and could in principle disagree with that snapshot; a
/// refusal that slips through there is still caught one spawn attempt later
/// by the startup-refusal classification in `crash_classification`.
pub fn require_sandbox_prerequisites(work_dir: &Path) -> Result<()> {
    let sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)?.unwrap_or_default();
    if !sandbox.enabled {
        return Ok(());
    }
    if !cfg!(target_os = "linux") {
        return Ok(());
    }

    let proc_version = std::fs::read_to_string("/proc/version").ok();
    let has_bwrap = which::which("bwrap").is_ok();
    let has_socat = which::which("socat").is_ok();

    if let Some(problem) =
        sandbox_prerequisite_problem(proc_version.as_deref(), has_bwrap, has_socat)
    {
        bail!("{problem}");
    }
    Ok(())
}

/// Pure decision function for [`require_sandbox_prerequisites`], so its
/// wording and branch order are unit-testable without depending on the
/// machine's actual kernel string or installed binaries.
///
/// WSL1 vs WSL2 is decided by two signals, not one: the kernel string
/// contains `microsoft` AND contains neither `microsoft-standard` (the config
/// every WSL2 kernel is built from) nor `wsl2` (present in the version suffix
/// of most, but not all, WSL2 builds). A WSL2 kernel older than 5.10.16
/// carries `microsoft-standard` without a `WSL2` substring at all — e.g.
/// `Linux version 4.19.128-microsoft-standard (oe-user@oe-host) ...` — so
/// checking `wsl2` alone would misclassify that host as WSL1 and refuse it.
pub(super) fn sandbox_prerequisite_problem(
    proc_version: Option<&str>,
    has_bwrap: bool,
    has_socat: bool,
) -> Option<String> {
    if let Some(version) = proc_version {
        let lower = version.to_lowercase();
        if lower.contains("microsoft")
            && !lower.contains("microsoft-standard")
            && !lower.contains("wsl2")
        {
            return Some(
                "This is WSL1 and Claude Code's sandbox requires WSL2. The plan sandbox is \
                 enabled and loom sets sandbox.failIfUnavailable, so every session would exit \
                 at startup. Convert the distro (`wsl --set-version <distro> 2` from Windows) \
                 or disable the sandbox in the plan."
                    .to_string(),
            );
        }
    }

    let mut missing: Vec<&str> = Vec::new();
    if !has_bwrap {
        missing.push("bubblewrap (bwrap)");
    }
    if !has_socat {
        missing.push("socat");
    }
    if missing.is_empty() {
        return None;
    }

    Some(format!(
        "Claude Code's Linux sandbox needs {}. The plan sandbox is enabled and loom sets \
         sandbox.failIfUnavailable, so every session would exit at startup with 'sandbox \
         required but unavailable'. Install them (apt install bubblewrap socat) and run loom \
         again, or disable the sandbox in the plan.",
        missing.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::sandbox_prerequisite_problem;

    const WSL1_VERSION: &str =
        "Linux version 4.4.0-19041-Microsoft (Microsoft@Microsoft.com) (gcc version 5.4.0)";
    const WSL2_VERSION: &str =
        "Linux version 5.15.167.4-microsoft-standard-WSL2 (root@buildkitsandbox) (gcc)";
    const PLAIN_LINUX_VERSION: &str =
        "Linux version 6.8.0-45-generic (buildd@lcy02) (gcc (Ubuntu 13.2.0-23ubuntu4) 13.2.0)";
    /// A WSL2 kernel older than 5.10.16: `microsoft-standard` with no `WSL2`
    /// substring anywhere in the string.
    const OLD_WSL2_VERSION: &str =
        "Linux version 4.19.128-microsoft-standard (oe-user@oe-host) (gcc version 8.2.0)";

    #[test]
    fn wsl1_with_both_tools_present_still_names_wsl2() {
        let problem = sandbox_prerequisite_problem(Some(WSL1_VERSION), true, true);
        let message = problem.expect("WSL1 kernel must be refused even with tools installed");
        assert!(message.contains("WSL2"), "message was: {message}");
    }

    #[test]
    fn wsl2_with_both_tools_is_fine() {
        assert_eq!(
            sandbox_prerequisite_problem(Some(WSL2_VERSION), true, true),
            None
        );
    }

    #[test]
    fn old_wsl2_kernel_without_a_wsl2_substring_is_fine() {
        assert_eq!(
            sandbox_prerequisite_problem(Some(OLD_WSL2_VERSION), true, true),
            None
        );
    }

    #[test]
    fn plain_linux_missing_only_socat_names_socat_not_bwrap() {
        let problem = sandbox_prerequisite_problem(Some(PLAIN_LINUX_VERSION), true, false);
        let message = problem.expect("missing socat must be refused");
        assert!(message.contains("socat"), "message was: {message}");
        assert!(!message.contains("bwrap"), "message was: {message}");
    }

    #[test]
    fn plain_linux_missing_both_tools_names_both() {
        let problem = sandbox_prerequisite_problem(Some(PLAIN_LINUX_VERSION), false, false);
        let message = problem.expect("missing both tools must be refused");
        assert!(message.contains("bwrap"), "message was: {message}");
        assert!(message.contains("socat"), "message was: {message}");
    }

    #[test]
    fn no_proc_version_with_both_tools_is_fine() {
        assert_eq!(sandbox_prerequisite_problem(None, true, true), None);
    }

    #[test]
    fn wsl1_and_missing_tools_the_wsl1_message_wins() {
        let problem = sandbox_prerequisite_problem(Some(WSL1_VERSION), false, false);
        let message = problem.expect("WSL1 with missing tools must be refused");
        assert!(message.contains("WSL2"), "message was: {message}");
        assert!(
            !message.contains("bubblewrap"),
            "WSL1 message must win over the missing-tools message: {message}"
        );
    }
}
