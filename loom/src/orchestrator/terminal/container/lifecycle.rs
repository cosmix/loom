//! Container lifecycle helpers — `run` arg construction.
//!
//! The container backend assembles `docker|podman|container run -d` from
//! the project-level execution config plus per-stage mounts and env. This
//! module is the pure-function builder; running the command, polling
//! `inspect`, and writing PID files lives in `mod.rs`.

use std::path::{Path, PathBuf};

use super::runtime::Runtime;

/// A single bind mount.
///
/// We always emit `--mount=type=bind,source=...,target=...` instead of `-v`
/// because Apple Container only supports the long form; portability across
/// all three runtimes outweighs `-v`'s brevity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub read_only: bool,
}

impl Mount {
    pub fn rw(source: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            read_only: false,
        }
    }

    pub fn ro(source: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            read_only: true,
        }
    }

    fn render(&self) -> String {
        let mut s = format!(
            "--mount=type=bind,source={},target={}",
            self.source.display(),
            self.target.display()
        );
        if self.read_only {
            s.push_str(",readonly");
        }
        s
    }
}

/// Build the full argv (excluding the runtime binary itself) for
/// `<runtime> run -d ...`.
///
/// `wrapper_in_container` is the in-container path to the wrapper script
/// that the entrypoint executes — typically
/// `/repo/.work/wrappers/<stage>-wrapper.sh`.
///
/// Containers are intentionally NOT started with `--rm`: they persist after
/// the wrapper exits so the orchestrator can capture `<runtime> logs` for
/// crash diagnostics. Removal is explicit in `kill_session` (after the
/// log tail has been persisted to `<work_dir>/crashes/`).
#[allow(clippy::too_many_arguments)]
pub fn build_run_args(
    name: &str,
    image_ref: &str,
    mounts: &[Mount],
    env_set: &[(String, String)],
    env_strip: &[&str],
    network: &str,
    runtime: Runtime,
    wrapper_in_container: &Path,
) -> Vec<String> {
    // Capability set:
    // - NET_ADMIN + NET_RAW: required by firewall.sh (iptables / ipset).
    // - SETUID + SETGID: required by gosu in entrypoint.sh to drop from
    //   root (firewall install) down to the unprivileged `loom` user
    //   (agent execution). Without these, `gosu loom "$@"` fails with
    //   "operation not permitted" and the container exits before the
    //   wrapper ever runs — `podman logs` then surfaces only that gosu
    //   error and the stage stalls with no claude output.
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "-d".to_string(),
        format!("--name={name}"),
        "--cap-drop=ALL".to_string(),
        "--cap-add=NET_ADMIN".to_string(),
        "--cap-add=NET_RAW".to_string(),
        "--cap-add=SETUID".to_string(),
        "--cap-add=SETGID".to_string(),
        format!("--network={network}"),
    ];

    args.extend(runtime.user_args());

    for mount in mounts {
        args.push(mount.render());
    }

    // Strip first — the container starts with these unset.
    for key in env_strip {
        args.push("-e".to_string());
        args.push(format!("{key}="));
    }

    // Then populate. The pair form (`-e KEY=value`) keeps the runtime parsing
    // happy across Docker / Podman / Apple Container.
    for (key, value) in env_set {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }

    args.push(image_ref.to_string());
    args.push(wrapper_in_container.display().to_string());

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_mounts() -> Vec<Mount> {
        vec![
            Mount::rw("/home/dev/loom", "/repo"),
            Mount::ro(
                "/home/dev/loom/.work/network/allowed_domains.txt",
                "/etc/loom/network/allowed_domains.txt",
            ),
            Mount::ro(
                "/home/dev/.claude/hooks/loom",
                "/home/loom/.claude/hooks/loom",
            ),
        ]
    }

    fn fixture_env_set() -> Vec<(String, String)> {
        vec![
            ("LOOM_SESSION_ID".to_string(), "session-x".to_string()),
            ("LOOM_STAGE_ID".to_string(), "stage-x".to_string()),
            ("LOOM_WORK_DIR".to_string(), "/repo/.work".to_string()),
            (
                "LOOM_WORKTREE_PATH".to_string(),
                "/repo/.worktrees/stage-x".to_string(),
            ),
            (
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                "1".to_string(),
            ),
        ]
    }

    const ENV_STRIP: &[&str] = &["SSH_AUTH_SOCK", "GH_TOKEN", "GITHUB_TOKEN"];

    #[test]
    fn docker_snapshot() {
        let args = build_run_args(
            "loom-stage-x",
            "sha256:deadbeef",
            &fixture_mounts(),
            &fixture_env_set(),
            ENV_STRIP,
            "loom-net-stage-x",
            Runtime::Docker,
            Path::new("/repo/.work/wrappers/stage-x-wrapper.sh"),
        );

        // Spot-check structure — the first few args are deterministic.
        // Note: no `--rm` — containers persist after exit so logs can be
        // captured before explicit removal in `kill_session`.
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "-d");
        assert!(!args.contains(&"--rm".to_string()));
        assert_eq!(args[2], "--name=loom-stage-x");
        assert_eq!(args[3], "--cap-drop=ALL");
        assert_eq!(args[4], "--cap-add=NET_ADMIN");
        assert_eq!(args[5], "--cap-add=NET_RAW");
        // SETUID + SETGID are required for gosu privilege drop in entrypoint.sh.
        assert_eq!(args[6], "--cap-add=SETUID");
        assert_eq!(args[7], "--cap-add=SETGID");
        assert_eq!(args[8], "--network=loom-net-stage-x");
        // Docker injects --user=uid:gid as its single user arg
        assert!(args[9].starts_with("--user="));
        // Mounts use the long form
        assert!(args
            .iter()
            .any(|a| a.starts_with("--mount=type=bind,source=/home/dev/loom,target=/repo")));
        // Strips appear as paired -e KEY=
        let pos_strip = args.iter().position(|a| a == "SSH_AUTH_SOCK=").unwrap();
        assert_eq!(args[pos_strip - 1], "-e");
        // Final two args: image then wrapper path
        let last = args.last().unwrap();
        assert_eq!(last, "/repo/.work/wrappers/stage-x-wrapper.sh");
        let penult = &args[args.len() - 2];
        assert_eq!(penult, "sha256:deadbeef");
    }

    #[test]
    fn podman_snapshot_uses_keep_id() {
        let args = build_run_args(
            "loom-stage-x",
            "sha256:deadbeef",
            &fixture_mounts(),
            &fixture_env_set(),
            ENV_STRIP,
            "loom-net-stage-x",
            Runtime::Podman,
            Path::new("/repo/.work/wrappers/stage-x-wrapper.sh"),
        );
        assert!(args.contains(&"--userns=keep-id".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--user=")));
    }

    #[test]
    fn apple_container_no_user_args() {
        let args = build_run_args(
            "loom-stage-x",
            "sha256:deadbeef",
            &fixture_mounts(),
            &fixture_env_set(),
            ENV_STRIP,
            "loom-net-stage-x",
            Runtime::AppleContainer,
            Path::new("/repo/.work/wrappers/stage-x-wrapper.sh"),
        );
        assert!(!args.iter().any(|a| a.starts_with("--userns")));
        assert!(!args.iter().any(|a| a.starts_with("--user=")));
    }

    #[test]
    fn ro_mount_includes_readonly() {
        let m = Mount::ro("/host/file", "/etc/file");
        let s = m.render();
        assert!(s.contains("readonly"));
    }

    #[test]
    fn rw_mount_omits_readonly() {
        let m = Mount::rw("/host", "/repo");
        let s = m.render();
        assert!(!s.contains("readonly"));
    }
}
