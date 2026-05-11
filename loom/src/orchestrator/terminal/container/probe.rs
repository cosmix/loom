//! Container firewall enforcement probe.
//!
//! Runs a transient container with the production-equivalent capability set
//! (`--cap-drop=ALL --cap-add=NET_ADMIN --cap-add=NET_RAW`) and an empty
//! allowlist, then attempts an outbound HTTPS request from the dropped-down
//! `loom` user. The smoke test PASSES when the request fails (firewall is
//! enforcing) and FAILS when the request succeeds (firewall is a no-op).
//!
//! The probe is invoked from `loom init` immediately after the image is
//! built. On runtimes where iptables-based egress filtering is best-effort
//! (rootless Podman without slirp4netns ≥ 1.2.3, Apple Container's limited
//! Linux capability emulation), the probe lets us refuse the configuration
//! with an actionable error rather than discovering the silent bypass once
//! stages start running.

use anyhow::{Context, Result};
use std::process::Command;

use super::runtime::Runtime;

/// Result of a firewall enforcement smoke test.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// `true` when an outbound request from inside the container was blocked
    /// (the firewall is enforcing). `false` when the request succeeded
    /// despite an empty allowlist — the firewall is not enforced.
    pub enforced: bool,
    /// Combined stdout+stderr of the probe container, suitable for surfacing
    /// to the user in an error message.
    pub diagnostic: String,
}

/// Inline shell run inside the probe container.
///
/// The probe must replicate production's privilege drop: the firewall script
/// is installed as root (entrypoint), then the agent process runs as the
/// unprivileged `loom` user. Curl from that user with `--max-time 3`. If the
/// firewall is enforcing, the connection is refused or times out and `curl`
/// exits non-zero. If it's NOT enforcing, curl exits 0 and we know the
/// runtime is permissive.
const PROBE_SHELL: &str = r#"set -u
/usr/local/bin/loom-firewall.sh >/tmp/loom-fw.log 2>&1 || true
# Run curl as the unprivileged loom user, matching production privilege drop.
# Exit code 0 from curl means the firewall DID NOT block the request.
if gosu loom curl --max-time 3 -sf https://1.1.1.1 >/dev/null 2>&1; then
  echo "PROBE_RESULT=bypassed"
  exit 0
else
  echo "PROBE_RESULT=blocked"
  exit 0
fi
"#;

/// Run the firewall enforcement smoke test against `image_ref` using `runtime`.
///
/// The container is spawned with the same capability matrix used in
/// production (`--cap-drop=ALL --cap-add=NET_ADMIN --cap-add=NET_RAW`).
/// Output is captured for the diagnostic field so the caller can surface
/// actionable details.
pub fn run_firewall_smoke_test(runtime: Runtime, image_ref: &str) -> Result<ProbeResult> {
    let mut cmd = Command::new(runtime.binary());
    cmd.arg("run")
        .arg("--rm")
        .arg("--cap-drop=ALL")
        .arg("--cap-add=NET_ADMIN")
        .arg("--cap-add=NET_RAW")
        // Probe runs as root; firewall.sh installs as root, then we gosu
        // down to loom for the curl attempt.
        .arg("--user=0:0")
        .arg("--entrypoint=/bin/bash")
        .arg(image_ref)
        .arg("-c")
        .arg(PROBE_SHELL);

    let output = cmd.output().with_context(|| {
        format!(
            "Failed to invoke `{} run` for firewall probe",
            runtime.binary()
        )
    })?;

    let mut diagnostic = String::new();
    diagnostic.push_str(&String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !diagnostic.is_empty() && !diagnostic.ends_with('\n') {
            diagnostic.push('\n');
        }
        diagnostic.push_str(&stderr);
    }

    let enforced = output.status.success() && diagnostic.contains("PROBE_RESULT=blocked");
    Ok(ProbeResult {
        enforced,
        diagnostic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_result_blocked_marker_string() {
        // Guard the contract that the probe shell emits a stable marker
        // line — error formatting and detection both depend on this.
        assert!(PROBE_SHELL.contains("PROBE_RESULT=blocked"));
        assert!(PROBE_SHELL.contains("PROBE_RESULT=bypassed"));
    }

    #[test]
    fn probe_result_struct_constructs() {
        let r = ProbeResult {
            enforced: true,
            diagnostic: "ok".to_string(),
        };
        assert!(r.enforced);
        assert_eq!(r.diagnostic, "ok");
    }
}
