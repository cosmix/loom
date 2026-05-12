//! Container runtime detection (Docker, Podman, Apple Container).
//!
//! Loom supports three container runtimes:
//!   * **Docker** — Linux + macOS, baseline option.
//!   * **Podman** — Linux preferred (rootless via `--userns=keep-id`).
//!   * **Apple Container** — macOS-only native runtime at
//!     `/usr/local/bin/container`.
//!
//! Detection priority:
//!   * Linux: Podman > Docker.
//!   * macOS: Apple Container > Podman > Docker.
//!
//! An explicit preference (`"docker" | "podman" | "container" | "auto"`)
//! overrides the platform default; "auto" walks the priority list.

use anyhow::{anyhow, bail, Result};
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

/// A detected container runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Docker,
    Podman,
    /// macOS-only native runtime.
    AppleContainer,
}

impl Runtime {
    /// Binary name to invoke for this runtime.
    pub fn binary(&self) -> &'static str {
        match self {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
            Runtime::AppleContainer => "container",
        }
    }

    /// Map a persisted runtime binary name (e.g. `"docker"`, `"podman"`,
    /// `"container"`) back to a [`Runtime`] variant. Returns `None` for
    /// unrecognised values so callers can fall back to a default.
    pub fn from_binary(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "docker" => Some(Runtime::Docker),
            "podman" => Some(Runtime::Podman),
            "container" => Some(Runtime::AppleContainer),
            _ => None,
        }
    }

    /// User-mapping arguments to apply to `<runtime> run`.
    ///
    /// * Podman uses `--userns=keep-id` so files written by the rootless
    ///   container appear owned by the host UID without manual chowning.
    /// * Docker requires explicit `--user=<uid>:<gid>` because the daemon
    ///   defaults to root inside the container, which would create
    ///   root-owned files in the bind-mounted worktree.
    /// * Apple Container handles UID/GID mapping itself; no extra args.
    pub fn user_args(&self) -> Vec<String> {
        match self {
            Runtime::Podman => vec!["--userns=keep-id".to_string()],
            Runtime::Docker => {
                #[cfg(unix)]
                {
                    // SAFETY: getuid/getgid never fail and have no preconditions.
                    let uid = unsafe { libc::getuid() };
                    let gid = unsafe { libc::getgid() };
                    vec![format!("--user={uid}:{gid}")]
                }
                #[cfg(not(unix))]
                {
                    Vec::new()
                }
            }
            Runtime::AppleContainer => Vec::new(),
        }
    }
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary())
    }
}

/// Detect a container runtime according to `preference`.
///
/// Accepts `"docker" | "podman" | "container" | "auto"`. `"auto"` walks the
/// platform priority list.
pub fn detect_runtime(preference: &str) -> Result<Runtime> {
    match preference.trim().to_ascii_lowercase().as_str() {
        "docker" => {
            if binary_in_path("docker") {
                Ok(Runtime::Docker)
            } else {
                bail!(install_hint(Runtime::Docker))
            }
        }
        "podman" => {
            if binary_in_path("podman") {
                Ok(Runtime::Podman)
            } else {
                bail!(install_hint(Runtime::Podman))
            }
        }
        "container" => {
            #[cfg(target_os = "macos")]
            {
                if is_apple_container() {
                    Ok(Runtime::AppleContainer)
                } else {
                    bail!(install_hint(Runtime::AppleContainer))
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                bail!("Apple Container runtime is only available on macOS")
            }
        }
        "auto" | "" => auto_detect(),
        other => Err(anyhow!(
            "Unknown container runtime preference: '{other}'. Expected one of: docker, podman, container, auto"
        )),
    }
}

#[cfg(target_os = "macos")]
fn auto_detect() -> Result<Runtime> {
    if is_apple_container() {
        return Ok(Runtime::AppleContainer);
    }
    if binary_in_path("podman") {
        return Ok(Runtime::Podman);
    }
    if binary_in_path("docker") {
        return Ok(Runtime::Docker);
    }
    Err(anyhow!(
        "No container runtime found. Install one of:\n  \
         * Apple Container: https://github.com/apple/container\n  \
         * Podman: brew install podman && podman machine init && podman machine start\n  \
         * Docker Desktop: https://www.docker.com/products/docker-desktop"
    ))
}

#[cfg(not(target_os = "macos"))]
fn auto_detect() -> Result<Runtime> {
    if binary_in_path("podman") {
        return Ok(Runtime::Podman);
    }
    if binary_in_path("docker") {
        return Ok(Runtime::Docker);
    }
    Err(anyhow!(
        "No container runtime found. Install one of:\n  \
         * Podman: https://podman.io/getting-started/installation\n  \
         * Docker: https://docs.docker.com/engine/install/"
    ))
}

/// Apple Container detection — verifies that `/usr/local/bin/container`
/// exists AND that Apple's code signature is present.
///
/// Why codesign rather than `--version` string matching: many systems ship
/// unrelated utilities at `/usr/local/bin/container`, and an attacker who
/// can drop a file at that path could trivially print "Apple" / "container"
/// strings to satisfy a string heuristic. `codesign -dvvv` consults the
/// kernel's notarisation database and the binary's embedded signature,
/// both of which require the Apple signing key the operator doesn't have.
///
/// We require BOTH:
///   1. The binary at the canonical path is signed.
///   2. The signing authority chain contains `Apple` (Apple Code Signing,
///      Apple Worldwide Developer Relations, etc.).
///
/// If `codesign` is itself unavailable (Apple ships it in macOS, but a
/// stripped-down host may not have it), we fail closed — better to reject
/// the runtime than to trust a bare path.
#[cfg(target_os = "macos")]
fn is_apple_container() -> bool {
    let canonical = Path::new("/usr/local/bin/container");
    if !canonical.exists() {
        return false;
    }

    // `codesign -dvvv` writes everything to stderr; stdout is empty.
    let output = match Command::new("/usr/bin/codesign")
        .args(["-dvvv", "--strict"])
        .arg(canonical)
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Apple-signed binaries list one or more `Authority=` lines naming
    // Apple-issued certificates. Reject if none mention Apple.
    let has_apple_authority = stderr
        .lines()
        .any(|l| l.starts_with("Authority=") && l.contains("Apple"));
    if !has_apple_authority {
        return false;
    }

    // Sanity check: the binary should also identify itself as `container`
    // (or a TeamIdentifier / Identifier line consistent with the Apple
    // Container product). We accept either Apple's product identifier or
    // the literal `container` token.
    let claims_container = stderr.lines().any(|l| {
        (l.starts_with("Identifier=") || l.starts_with("TeamIdentifier="))
            && (l.to_ascii_lowercase().contains("container")
                || l.contains("Apple"))
    });
    if !claims_container {
        return false;
    }

    true
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn is_apple_container() -> bool {
    false
}

fn binary_in_path(name: &str) -> bool {
    let Some(path_env) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

fn install_hint(runtime: Runtime) -> String {
    match runtime {
        Runtime::Docker => {
            if cfg!(target_os = "macos") {
                "Docker not found. Install via:\n  * Docker Desktop: https://www.docker.com/products/docker-desktop\n  * Or `brew install --cask docker`"
                    .to_string()
            } else {
                "Docker not found. Install via your package manager or https://docs.docker.com/engine/install/"
                    .to_string()
            }
        }
        Runtime::Podman => {
            if cfg!(target_os = "macos") {
                "Podman not found. Install via:\n  brew install podman && podman machine init && podman machine start"
                    .to_string()
            } else {
                "Podman not found. Install via your package manager (e.g. `apt install podman` / `dnf install podman`)"
                    .to_string()
            }
        }
        Runtime::AppleContainer => {
            "Apple Container runtime not found at /usr/local/bin/container.\n  \
             Install from https://github.com/apple/container"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_names() {
        assert_eq!(Runtime::Docker.binary(), "docker");
        assert_eq!(Runtime::Podman.binary(), "podman");
        assert_eq!(Runtime::AppleContainer.binary(), "container");
    }

    #[test]
    fn podman_keep_id() {
        let args = Runtime::Podman.user_args();
        assert_eq!(args, vec!["--userns=keep-id".to_string()]);
    }

    #[test]
    fn docker_user_args_present() {
        let args = Runtime::Docker.user_args();
        assert_eq!(args.len(), 1);
        assert!(args[0].starts_with("--user="));
        // Should contain a colon between uid:gid
        assert!(args[0].contains(':'));
    }

    #[test]
    fn apple_container_no_user_args() {
        assert!(Runtime::AppleContainer.user_args().is_empty());
    }

    #[test]
    fn detect_runtime_rejects_unknown() {
        let err = detect_runtime("rocket").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Unknown container runtime preference"));
    }

    #[test]
    fn detect_runtime_auto_consistent_with_platform() {
        // We can't assert which runtime exists on the test machine, but
        // `auto` must either pick a real runtime or return a helpful error.
        match detect_runtime("auto") {
            Ok(_) => {}
            Err(e) => {
                let s = format!("{e}");
                assert!(
                    s.contains("Install") || s.contains("install"),
                    "auto-detect failure should be actionable: {s}"
                );
            }
        }
    }
}
