//! Container terminal backend.
//!
//! Spawns Claude Code sessions inside a managed Docker/Podman/Apple
//! Container so each stage runs in an isolated filesystem + network
//! namespace.
//!
//! **Topology (project-level invariant — see plan):**
//!   * `<host repo root>` -> `/repo` (rw bind mount). Preserves git
//!     worktree metadata + the relative `.work` symlink + host-absolute
//!     hook references.
//!   * Stage cwd: `/repo/.worktrees/<stage_id>`.
//!   * Merge / base-conflict / knowledge cwd: `/repo`.
//!   * `LOOM_WORK_DIR=/repo/.work`.
//!   * `~/.claude/hooks/loom` -> `/home/loom/.claude/hooks/loom` (ro).
//!   * `~/.claude/.credentials.json` -> `/home/loom/.claude/.credentials.json`
//!     (ro, only when `forward_credentials` contains `"claude"`).
//!   * `<host>/.work/network/allowed_domains.txt` ->
//!     `/etc/loom/network/allowed_domains.txt` (ro). The full firewall +
//!     allowlist sidecar lands in stage 4.
//!
//! Liveness uses `<runtime> inspect -f '{{.State.Running}}'`; we never
//! `kill -0` against the host PID for container sessions.

pub mod fingerprint;
pub mod image;
pub mod lifecycle;
pub mod network;
pub mod resources;
pub mod runtime;

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use lifecycle::{build_run_args, Mount};
use runtime::{detect_runtime, Runtime};

use super::native::{
    create_wrapper_script_with_paths, detect_terminal, spawn_in_terminal, WrapperPaths,
};
use super::{BackendType, TerminalBackend};
use crate::claude::find_claude_path;
use crate::fs::work_dir as work_dir_api;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::plan::schema::execution::ProjectExecutionConfig;
use crate::plan::schema::NetworkConfig;
use shell_escape::escape;
use std::borrow::Cow;

/// Container-stable mountpoint for the host repo root.
const REPO_MOUNT: &str = "/repo";
/// Container-stable .work mountpoint (derived from `REPO_MOUNT`).
const WORK_DIR_IN_CONTAINER: &str = "/repo/.work";
/// Container-stable hooks mountpoint.
const HOOKS_MOUNT: &str = "/home/loom/.claude/hooks/loom";
/// Container-stable Claude credentials mountpoint (when forwarded).
const CLAUDE_CREDS_MOUNT: &str = "/home/loom/.claude/.credentials.json";
/// Container-stable network allowlist mountpoint (read in stage 4).
const ALLOWLIST_MOUNT: &str = "/etc/loom/network/allowed_domains.txt";

/// Env keys stripped at container launch. These leak host credentials or
/// agent metadata that the containerised process must not inherit.
const ENV_STRIP: &[&str] = &["SSH_AUTH_SOCK", "GH_TOKEN", "GITHUB_TOKEN"];

/// Container terminal backend.
pub struct ContainerBackend {
    runtime: Runtime,
    work_dir: PathBuf,
    image_ref: String,
    forward_credentials: Vec<String>,
    /// Plan-level network policy. Cached at construction so each spawn
    /// can materialise the allowlist file without re-reading
    /// `.work/config.toml` on every call.
    network: NetworkConfig,
}

impl ContainerBackend {
    /// Build a `ContainerBackend` from the project's persisted execution
    /// config.
    ///
    /// Reads `<work_dir>/config.toml::[project_execution]` and refuses to
    /// instantiate unless an image digest has been pinned (the digest is
    /// written by `loom init --backend container` once the image is built).
    pub fn new(work_dir: PathBuf) -> Result<Self> {
        let project = work_dir_api::read_project_execution(&work_dir)
            .context("Failed to read [project_execution] from .work/config.toml")?
            .ok_or_else(|| {
                anyhow!(
                    "No [project_execution] section in .work/config.toml. \
                     Run `loom init --backend container` to provision the container backend."
                )
            })?;

        let container = project.container.as_ref().ok_or_else(|| {
            anyhow!(
                "Project backend is configured but [project_execution.container] is missing. \
                 Run `loom init --backend container` to provision a container image."
            )
        })?;

        let digest = container.image_digest.trim();
        if digest.is_empty() || digest == "pending" {
            bail!("expected execution.container.image_digest; run loom init --backend container");
        }

        let runtime = detect_runtime("auto").context("Container runtime detection failed")?;

        // Read plan-level network policy once at construction. Missing
        // section falls back to defaults (empty allowlist — firewall
        // rejects everything beyond the hardcoded ALWAYS list).
        let network = work_dir_api::read_plan_sandbox(&work_dir)
            .context("Failed to read [plan_sandbox] from .work/config.toml")?
            .map(|s| s.network)
            .unwrap_or_default();

        Ok(Self {
            runtime,
            work_dir,
            image_ref: digest.to_string(),
            forward_credentials: container.forward_credentials.clone(),
            network,
        })
    }

    /// Construct from already-resolved fields (used by tests and call
    /// sites that have just read [`ProjectExecutionConfig`] for other
    /// reasons; avoids re-reading the config).
    #[allow(dead_code)]
    pub fn from_project(
        runtime: Runtime,
        work_dir: PathBuf,
        project: &ProjectExecutionConfig,
    ) -> Result<Self> {
        let container = project
            .container
            .as_ref()
            .ok_or_else(|| anyhow!("project_execution.container missing"))?;
        let digest = container.image_digest.trim();
        if digest.is_empty() || digest == "pending" {
            bail!("expected execution.container.image_digest; run loom init --backend container");
        }
        let network = work_dir_api::read_plan_sandbox(&work_dir)
            .context("Failed to read [plan_sandbox] from .work/config.toml")?
            .map(|s| s.network)
            .unwrap_or_default();
        Ok(Self {
            runtime,
            work_dir,
            image_ref: digest.to_string(),
            forward_credentials: container.forward_credentials.clone(),
            network,
        })
    }

    fn host_repo_root(&self) -> Result<PathBuf> {
        // `.work` lives at the repo root in the main repo. In worktrees
        // `.work` is a symlink that resolves to the main repo's `.work`.
        // Either way, the parent of the resolved `.work` is the repo root.
        let resolved = self
            .work_dir
            .canonicalize()
            .unwrap_or_else(|_| self.work_dir.clone());
        resolved.parent().map(|p| p.to_path_buf()).ok_or_else(|| {
            anyhow!(
                "Could not determine host repo root from {}",
                resolved.display()
            )
        })
    }

    fn build_mounts(&self) -> Result<Vec<Mount>> {
        let mut mounts = Vec::with_capacity(4);
        mounts.push(Mount::rw(self.host_repo_root()?, REPO_MOUNT));

        // Hooks (read-only) — only if installed.
        if let Some(home) = dirs::home_dir() {
            let hooks_src = home.join(".claude/hooks/loom");
            if hooks_src.exists() {
                mounts.push(Mount::ro(hooks_src, HOOKS_MOUNT));
            }

            // Optional credentials — only when explicitly forwarded.
            if self
                .forward_credentials
                .iter()
                .any(|c| c.eq_ignore_ascii_case("claude"))
            {
                let creds = home.join(".claude/.credentials.json");
                if creds.exists() {
                    mounts.push(Mount::ro(creds, CLAUDE_CREDS_MOUNT));
                }
            }
        }

        // Network allowlist (read-only). The file may not exist yet at
        // stage 3; mounting a non-existent source fails, so skip silently.
        let allowlist_src = self.work_dir.join("network").join("allowed_domains.txt");
        if allowlist_src.exists() {
            mounts.push(Mount::ro(allowlist_src, ALLOWLIST_MOUNT));
        }

        Ok(mounts)
    }

    fn build_env_for_session(
        &self,
        stage_id: &str,
        session_id: &str,
        workspace_in_container: &Path,
    ) -> Vec<(String, String)> {
        vec![
            ("LOOM_SESSION_ID".to_string(), session_id.to_string()),
            ("LOOM_STAGE_ID".to_string(), stage_id.to_string()),
            (
                "LOOM_WORK_DIR".to_string(),
                WORK_DIR_IN_CONTAINER.to_string(),
            ),
            (
                "LOOM_WORKTREE_PATH".to_string(),
                workspace_in_container.display().to_string(),
            ),
            (
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                "1".to_string(),
            ),
            // git refuses to ask interactively for credentials inside the
            // container by routing askpass at /bin/false.
            ("GIT_ASKPASS".to_string(), "/bin/false".to_string()),
        ]
    }

    fn network_name(&self, stage_id: &str) -> String {
        format!("loom-net-{stage_id}")
    }

    /// Spawn flow shared by all four session variants. Returns the spawned
    /// session with `pid` (Docker/Podman), `container_name`, `runtime`
    /// populated and `try_mark_running()` applied.
    #[allow(clippy::too_many_arguments)]
    fn spawn_common(
        &self,
        stage: &Stage,
        session: Session,
        workspace_in_container: PathBuf,
        signal_prompt: String,
        title: &str,
        no_attach: bool,
        model: &str,
        effort: &str,
    ) -> Result<Session> {
        let mut session = session;
        let tracking_key = if session.tracking_key.is_empty() {
            Session::derive_tracking_key(&stage.id, session.session_type)
        } else {
            session.tracking_key.clone()
        };
        let container_name = tracking_key.clone();
        session.tracking_key = tracking_key.clone();

        // Materialise the per-stage allowlist file before launch so the
        // in-container firewall script has a populated policy to read
        // (mounted RO at /etc/loom/network/allowed_domains.txt).
        network::write_allowlist(&self.work_dir, &self.network)
            .with_context(|| format!("Failed to write network allowlist for stage {}", stage.id))?;

        let network = self.network_name(&stage.id);
        network::ensure_network(&self.runtime, &stage.id).with_context(|| {
            format!("Failed to create container network for stage {}", stage.id)
        })?;

        // Build the claude command (escaped) — same shape as the native
        // backend so signal-file UX matches.
        let claude_path = find_claude_path()?;
        let escaped_prompt = escape(Cow::Borrowed(signal_prompt.as_str()));
        let claude_cmd = format!(
            "{} --model {} --effort {} {escaped_prompt}",
            claude_path.display(),
            escape(Cow::Borrowed(model)),
            effort
        );

        // Generate the wrapper script with container-relative paths.
        let paths = WrapperPaths {
            work_dir_in_container: PathBuf::from(WORK_DIR_IN_CONTAINER),
            workspace_in_container: Some(workspace_in_container.clone()),
        };
        let wrapper_host_path = create_wrapper_script_with_paths(
            &self.work_dir,
            &stage.id,
            &session.id,
            &claude_cmd,
            None,
            Some(&paths),
        )?;
        // Wrapper script lives at <host>/.work/wrappers/<stage>-wrapper.sh
        // which maps to <REPO_MOUNT>/.work/wrappers/<stage>-wrapper.sh.
        let wrapper_filename = wrapper_host_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Wrapper script path missing filename"))?;
        let wrapper_in_container = PathBuf::from(format!(
            "{WORK_DIR_IN_CONTAINER}/wrappers/{wrapper_filename}"
        ));

        // Compose the env block and run-args.
        let env_set = self.build_env_for_session(&stage.id, &session.id, &workspace_in_container);
        let mounts = self.build_mounts()?;
        let args = build_run_args(
            &container_name,
            &self.image_ref,
            &mounts,
            &env_set,
            ENV_STRIP,
            &network,
            self.runtime,
            &wrapper_in_container,
        );

        // Detached start (`run -d`) returns immediately. We then poll
        // `inspect` until `State.Running == true`.
        let output = Command::new(self.runtime.binary())
            .args(&args)
            .output()
            .with_context(|| {
                format!(
                    "Failed to invoke `{} run` for stage {}",
                    self.runtime.binary(),
                    stage.id
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "`{} run` failed for stage {}: {}",
                self.runtime.binary(),
                stage.id,
                stderr.trim()
            );
        }

        wait_until_running(self.runtime, &container_name, Duration::from_secs(10))?;

        // Capture container PID (Docker/Podman) and persist the host-side
        // pid file at <work_dir>/pids/<stage>.pid so the monitor's legacy
        // file-based liveness checks still see something sensible during
        // the migration. is_session_alive bypasses the file via
        // LivenessService.
        if self.runtime != Runtime::AppleContainer {
            if let Some(pid) = inspect_pid(self.runtime, &container_name)? {
                let pid_file = self.work_dir.join("pids").join(format!("{}.pid", stage.id));
                if let Some(parent) = pid_file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&pid_file, pid.to_string());
                session.set_pid(pid);
            }
        }

        session.set_container_identity(self.runtime.binary().to_string(), container_name.clone());
        session.set_backend(BackendType::Container);
        session.try_mark_running()?;

        // Optional: attach a host terminal that tails the session log.
        if !no_attach {
            // Best-effort: failure to attach must not roll back the
            // container spawn.
            if let Ok(terminal) = detect_terminal() {
                let log_in_container = format!("/repo/.work/sessions/{}.log", session.id);
                let escaped_log = escape(Cow::Owned(log_in_container));
                let exec_cmd = format!(
                    "{rt} exec -it {name} /bin/bash -lc 'tail -f {escaped_log}'",
                    rt = self.runtime.binary(),
                    name = escape(Cow::Borrowed(&container_name)),
                    escaped_log = escaped_log,
                );
                // Tail terminals start in the host's repo root for parity
                // with native sessions; the actual container cwd is set
                // by the wrapper inside.
                let host_repo_root = self
                    .host_repo_root()
                    .unwrap_or_else(|_| self.work_dir.clone());
                let _ = spawn_in_terminal(
                    &terminal,
                    title,
                    &host_repo_root,
                    &exec_cmd,
                    Some(&self.work_dir),
                    Some(&stage.id),
                );
            }
        }

        Ok(session)
    }
}

impl TerminalBackend for ContainerBackend {
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        // Standard stage session: cwd = /repo/.worktrees/<stage>.
        let workspace = PathBuf::from(format!("{REPO_MOUNT}/.worktrees/{}", stage.id));
        let signal_in_container = remap_signal_path(signal_path);
        let prompt = format!(
            "Read the signal file at {signal} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read.",
            signal = signal_in_container.display(),
        );
        let title = format!("loom-{}", stage.id);
        let model = stage.effective_model().to_string();
        let effort = stage.effective_reasoning_effort().to_string();
        let session = self.spawn_common(
            stage, session, workspace, prompt, &title, false, &model, &effort,
        )?;

        // Mirror native backend: track host-side worktree path for the
        // session-record (used for status display).
        let mut session = session;
        session.set_worktree_path(worktree.path.clone());
        session.assign_to_stage(stage.id.clone());
        Ok(session)
    }

    fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        _repo_root: &Path,
    ) -> Result<Session> {
        let workspace = PathBuf::from(REPO_MOUNT);
        let signal_in_container = remap_signal_path(signal_path);
        let prompt = format!(
            "Read the merge signal file at {signal} and resolve the merge conflicts. \
             This file contains the conflicting files, merge context, and resolution instructions.",
            signal = signal_in_container.display(),
        );
        let title = format!("loom-merge-{}", stage.id);
        let session = self.spawn_common(
            stage, session, workspace, prompt, &title, false, "opus[1m]", "xhigh",
        )?;
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        Ok(session)
    }

    fn spawn_base_conflict_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        _repo_root: &Path,
    ) -> Result<Session> {
        let workspace = PathBuf::from(REPO_MOUNT);
        let signal_in_container = remap_signal_path(signal_path);
        let prompt = format!(
            "Read the base conflict signal file at {signal} and resolve the merge conflicts. \
             This file contains the conflicting files from merging dependency branches, \
             and instructions for resolution. After resolving, tell the user to run `loom retry {stage_id}`.",
            signal = signal_in_container.display(),
            stage_id = stage.id,
        );
        let title = format!("loom-base-conflict-{}", stage.id);
        let session = self.spawn_common(
            stage, session, workspace, prompt, &title, false, "opus[1m]", "xhigh",
        )?;
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        Ok(session)
    }

    fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        _repo_root: &Path,
    ) -> Result<Session> {
        let workspace = PathBuf::from(REPO_MOUNT);
        let signal_in_container = remap_signal_path(signal_path);
        let prompt = format!(
            "Read the signal file at {signal} and execute the assigned knowledge gathering work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and instructions for populating the knowledge base.",
            signal = signal_in_container.display(),
        );
        let title = format!("loom-knowledge-{}", stage.id);
        let model = stage.effective_model().to_string();
        let effort = stage.effective_reasoning_effort().to_string();
        let session = self.spawn_common(
            stage, session, workspace, prompt, &title, false, &model, &effort,
        )?;
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        Ok(session)
    }

    fn kill_session(&self, session: &Session) -> Result<()> {
        let name = match session.container_name.as_deref() {
            Some(n) => n,
            None => {
                // Nothing to kill — treat as already-gone.
                return Ok(());
            }
        };

        let output = Command::new(self.runtime.binary())
            .args(["rm", "-f", name])
            .output()
            .with_context(|| {
                format!("Failed to invoke `{} rm -f {name}`", self.runtime.binary())
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // "No such container" / "no such object" are non-fatal.
            let lower = stderr.to_ascii_lowercase();
            if !lower.contains("no such") {
                bail!(
                    "`{} rm -f {name}` failed: {}",
                    self.runtime.binary(),
                    stderr.trim()
                );
            }
        }

        if let Some(stage_id) = session.stage_id.as_deref() {
            let _ = network::remove_network(&self.runtime, stage_id);
            // Best-effort cleanup of the host-side pid file. Direct
            // removal — there is no native PID-tracking helper to call
            // here because the container backend does not own the
            // wrapper-script side files for native sessions.
            let _ =
                std::fs::remove_file(self.work_dir.join("pids").join(format!("{stage_id}.pid")));
        }
        Ok(())
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        let Some(name) = session.container_name.as_deref() else {
            return Ok(false);
        };
        inspect_running(self.runtime, name)
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Container
    }
}

/// Remap a host-side signal-file path onto the container mount.
///
/// Host `.work/signals/<id>.md` (under the repo root) maps to
/// `/repo/.work/signals/<id>.md`. If the path is already inside the
/// expected directory hierarchy we can splice on the suffix; otherwise we
/// fall back to leaving the host path untouched (the wrapper script also
/// has the host path via the bind mount).
fn remap_signal_path(signal_path: &Path) -> PathBuf {
    // Heuristic: find the ".work" component and rebuild from there.
    let mut components = signal_path.components().peekable();
    let mut suffix: Option<PathBuf> = None;
    while let Some(comp) = components.next() {
        if let std::path::Component::Normal(name) = comp {
            if name == ".work" {
                let mut tail = PathBuf::from(".work");
                for rest in components.by_ref() {
                    tail.push(rest.as_os_str());
                }
                suffix = Some(tail);
                break;
            }
        }
    }
    match suffix {
        Some(s) => PathBuf::from(REPO_MOUNT).join(s),
        None => signal_path.to_path_buf(),
    }
}

fn wait_until_running(runtime: Runtime, name: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if inspect_running(runtime, name)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "Container `{name}` did not reach Running state within {} seconds",
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn inspect_running(runtime: Runtime, name: &str) -> Result<bool> {
    let output = Command::new(runtime.binary())
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .output()
        .with_context(|| format!("Failed to invoke `{} inspect {name}`", runtime.binary()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("no such") {
            return Ok(false);
        }
        bail!(
            "`{} inspect {name}` failed: {}",
            runtime.binary(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "true")
}

fn inspect_pid(runtime: Runtime, name: &str) -> Result<Option<u32>> {
    let output = Command::new(runtime.binary())
        .args(["inspect", "-f", "{{.State.Pid}}", name])
        .output()
        .with_context(|| format!("Failed to invoke `{} inspect {name}`", runtime.binary()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout.parse::<u32>().ok().filter(|&p| p != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn remap_signal_handles_worktree_path() {
        let host = Path::new("/home/dev/loom/.worktrees/stage-x/.work/signals/session-x-1.md");
        let mapped = remap_signal_path(host);
        assert_eq!(mapped, PathBuf::from("/repo/.work/signals/session-x-1.md"));
    }

    #[test]
    fn remap_signal_handles_main_repo_path() {
        let host = Path::new("/home/dev/loom/.work/signals/abc.md");
        let mapped = remap_signal_path(host);
        assert_eq!(mapped, PathBuf::from("/repo/.work/signals/abc.md"));
    }

    #[test]
    fn remap_signal_passthrough_when_no_work() {
        let host = Path::new("/tmp/random.md");
        let mapped = remap_signal_path(host);
        assert_eq!(mapped, host);
    }
}
