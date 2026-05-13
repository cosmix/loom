//! Container terminal backend.
//!
//! Spawns Claude Code sessions inside a managed Docker/Podman/Apple
//! Container so each stage runs in an isolated filesystem + network
//! namespace.
//!
//! **Topology (project-level invariant — see plan):**
//!   * `<host repo root>` -> `/repo` (read-only base). Preserves git
//!     worktree metadata + the relative `.work` symlink + host-absolute
//!     hook references, but prevents the session from mutating files
//!     outside its assigned scope.
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
//! **Mount layering pattern (`build_mounts`):**
//!
//! The mount list is constructed as a stack of overlays. Later entries
//! shadow earlier ones — order is load-bearing, do not reorder.
//!
//! 1. **Base `/repo`** — read-only for ordinary stages (Standard,
//!    IntegrationVerify, Knowledge, KnowledgeDistill). Merge and
//!    BaseConflict sessions instead get a **broad rw `/repo` mount** as
//!    the documented exception: conflict resolution needs to touch
//!    arbitrary files, and there is no useful subtree to scope it to.
//! 2. **Per-stage rw scope** — Standard/IntegrationVerify get rw on
//!    `/repo/.worktrees/<id>`; Knowledge/KnowledgeDistill get rw on
//!    `/repo/doc/loom/knowledge`. Merge/BaseConflict skip this layer
//!    (their /repo is already rw).
//! 3. **`.work/` rw subtrees** — `sessions`, `memory`, `handoffs`,
//!    `crashes`, `wrappers`, `pids`, plus the hook observability paths
//!    `hooks`, `heartbeat`, `compaction-pending`, `compaction-recovery`,
//!    and the top-level append-only file `tool-events.jsonl`, are always
//!    rw for all sessions. Notably absent: `.work/config.toml`, `stages/`,
//!    `signals/`, `daemon.token`, `orchestrator.lock` — these stay ro
//!    under the `/repo` base so the agent cannot corrupt orchestration
//!    state.
//! 4. **`.claude/settings.local.json` ro overlay** — pinned read-only
//!    AFTER the worktree rw mount so the agent cannot rewrite its own
//!    permission grants mid-session.
//! 5. **Hooks dir + credentials** — existing ro mounts, unchanged.
//!
//! Liveness uses `<runtime> inspect -f '{{.State.Running}}'`; we never
//! `kill -0` against the host PID for container sessions.
//!
//! ## Container lifecycle
//!
//! Every container removal trigger is listed below. After removal, the session file
//! is updated to clear `container_name` / `runtime` so stale references do not
//! mislead future `loom container logs` / `loom container list` calls.
//!
//! - **Stage completes successfully** — `handle_stage_completed` (orchestrator/core/completion_handler.rs)
//!   calls `kill_session`, then clears container identity on the session file.
//! - **Stale merge / base-conflict session reap** — `merge_handler.rs` kills stale sessions
//!   and clears container identity.
//! - **`loom sessions kill <id>` or `loom sessions kill --stage <stage>`** — `commands/sessions.rs`
//!   calls `kill_session` then deletes the session file entirely (no clear needed).
//! - **Daemon shutdown (`loom stop`)** — reaps active container sessions via `kill_session`,
//!   then clears container identity on persisted session files.
//! - **`loom clean --sessions` / `--all`** — reaps orphan `loom-*` containers and
//!   `loom-net-*` networks left behind by a crashed daemon (best-effort bulk removal).
//! - **Spawn-time `wait_until_running` timeout** — logs captured to `.work/crashes/` before
//!   removal. The in-memory session was never persisted with container_name, so no file update.
//!
//! Parallel stages get independent containers, named by `Session::derive_tracking_key`.
//! They share only the per-stage `loom-net-<stage>` network and the immutable image.

pub mod fingerprint;
pub mod git_bridge;
pub mod image;
pub mod lifecycle;
pub mod logs_capture;
pub mod network;
pub mod probe;
pub mod resources;
pub mod runtime;

use anyhow::{anyhow, bail, Context, Result};
use std::os::unix::fs::OpenOptionsExt;
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
use crate::fs::work_dir as work_dir_api;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageType};
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
/// Container-stable mountpoint for the per-stage bare git mirror.
///
/// The host-side path under `<work_dir>/git-mirrors/<stage-id>/` (see
/// [`git_bridge::bare_mirror_path`]) is bind-mounted **read-only**
/// here. The container's entrypoint clones from this mirror into
/// `/repo` before exec-ing the agent. The agent CANNOT push to this
/// mirror — it is an RO bind mount, so any write attempt fails with
/// EROFS.
///
/// This is the architectural primitive that closes Codex blocker B1:
/// without a `.git` bind mount, an agent cannot tamper with host git
/// history, refs, or hooks.
pub const MIRROR_MOUNT: &str = "/var/loom/mirror";

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
    /// Git author/committer name injected via GIT_AUTHOR_NAME /
    /// GIT_COMMITTER_NAME into every container session. None means git
    /// falls back to its own defaults inside the container.
    git_user_name: Option<String>,
    /// Git author/committer email injected via GIT_AUTHOR_EMAIL /
    /// GIT_COMMITTER_EMAIL into every container session. None means git
    /// falls back to its own defaults inside the container.
    git_user_email: Option<String>,
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
            git_user_name: container.git_user_name.clone(),
            git_user_email: container.git_user_email.clone(),
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
            git_user_name: container.git_user_name.clone(),
            git_user_email: container.git_user_email.clone(),
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

    /// Construct the bind-mount stack for a single session.
    ///
    /// See the module-level "Mount layering pattern" doc for ordering
    /// invariants. The stack is layered ro-base → per-stage rw → .work/
    /// rw subtrees → settings.local ro overlay → hooks/creds/allowlist.
    fn build_mounts(
        &self,
        stage: &Stage,
        session_type: SessionType,
        session_id: &str,
    ) -> Result<Vec<Mount>> {
        let mut mounts: Vec<Mount> = Vec::with_capacity(16);
        let host_repo_root = self.host_repo_root()?;

        // Layer 1+2: base /repo + per-stage rw scope.
        //
        // Merge/BaseConflict are the documented exception: conflict
        // resolution must touch arbitrary files in arbitrary subtrees, so
        // we drop the ro base and grant rw on /repo. All other session
        // types pin /repo ro and add a narrow rw overlay on the subtree
        // the stage is allowed to mutate.
        // Knowledge/KnowledgeDistill stages need the same broad rw access
        // as Merge/BaseConflict because they:
        //   - commit directly to main (requires .git/ writable)
        //   - call `loom stage complete` (requires .work/stages/ writable)
        //   - run in the main repo, NOT an isolated worktree
        // The narrow `doc/loom/knowledge` rw overlay (from the original
        // hardening plan) was incompatible with these requirements:
        // `git commit` failed with "Read-only file system" on .git/index.lock
        // and `loom stage complete` failed writing the stage file. Container
        // isolation (cap-drop, firewall, network namespace, ro `~/.claude`)
        // remains the security boundary; file-level restrictions inside the
        // container can't enforce what the stage type fundamentally needs to do.
        let needs_full_rw_repo =
            matches!(session_type, SessionType::Merge | SessionType::BaseConflict)
                || matches!(
                    stage.stage_type,
                    StageType::Knowledge | StageType::KnowledgeDistill
                );
        if needs_full_rw_repo {
            mounts.push(Mount::rw(&host_repo_root, REPO_MOUNT));
        } else {
            mounts.push(Mount::ro(&host_repo_root, REPO_MOUNT));
            // Standard / IntegrationVerify use the narrow worktree overlay
            // PLUS targeted rw overlays for paths the agent needs to write
            // outside its worktree:
            //
            //   - /repo/.git — `git commit` from inside a worktree writes to
            //     the main repo's .git/ (the worktree's `.git` is just a
            //     file pointing at `../../.git/worktrees/<id>/`). Without
            //     this, the agent hits "Unable to create '.git/index.lock':
            //     Read-only file system" on every commit attempt.
            let host_wt = host_repo_root.join(".worktrees").join(&stage.id);
            let cont_wt = format!("{REPO_MOUNT}/.worktrees/{}", stage.id);
            mounts.push(Mount::rw(host_wt, cont_wt));

            let host_git = host_repo_root.join(".git");
            if host_git.exists() {
                mounts.push(Mount::rw(host_git, format!("{REPO_MOUNT}/.git")));
            }
        }

        // Layer 3: .work/ rw subtrees. Everything the session may need to
        // write under `.work` is enumerated explicitly so the ro base
        // continues to protect config.toml, signals/, sibling stage
        // files, the daemon token, and the orchestrator lock.
        //
        // Podman refuses to auto-create missing bind-mount sources
        // (Docker silently creates them as root-owned dirs, which would
        // defeat the rw overlay anyway). Ensure each source exists with
        // mode 0o755 before constructing the mount.
        //
        // NOTE: `stages` is intentionally NOT in this list. The whole
        // directory rw would let any stage's agent edit any other
        // stage's file (phantom-merge by writing `merged: true` directly).
        // The agent's only legitimate need is to update its OWN stage
        // file via `loom stage complete`, so we mount JUST that single
        // file rw below.
        //
        // The four `hooks`-related dirs (`hooks/`, `heartbeat/`,
        // `compaction-pending/`, `compaction-recovery/`) MUST be in this
        // list — Claude Code's session-start / session-end / pre-compact /
        // post-tool-use hooks all write into them, and without rw the
        // hook scripts fail with EROFS, dropping observability events
        // (events.jsonl, heartbeat markers, compaction recovery markers).
        for sub in [
            "sessions",
            "memory",
            "handoffs",
            "crashes",
            "wrappers",
            "pids",
            "hooks",
            "heartbeat",
            "compaction-pending",
            "compaction-recovery",
        ] {
            let host = self.work_dir.join(sub);
            std::fs::create_dir_all(&host).with_context(|| {
                format!(
                    "Failed to create rw mount source directory {} for container backend",
                    host.display()
                )
            })?;
            let cont = format!("{WORK_DIR_IN_CONTAINER}/{sub}");
            mounts.push(Mount::rw(host, cont));
        }

        // Top-level append-only file rw mount: `.work/tool-events.jsonl`.
        // Written by the `post-tool-use.sh` hook on every tool call and
        // read by the orchestrator's stuck-detection monitor
        // (`orchestrator/monitor/tool_analysis.rs`). Can't go under a dir
        // rw overlay because it sits directly under `.work/`, alongside
        // ro-protected files (config.toml, daemon.token, ...). Pre-create
        // the host file as empty 0o644 so the bind mount has a real source
        // — Podman won't auto-create file sources, and a missing source
        // would either fail the spawn (Podman) or silently materialize a
        // root-owned directory (Docker), neither of which is acceptable.
        let tool_events_host = self.work_dir.join("tool-events.jsonl");
        if !tool_events_host.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o644)
                .open(&tool_events_host)
                .with_context(|| {
                    format!(
                        "Failed to pre-create rw mount source file {} for container backend",
                        tool_events_host.display()
                    )
                })?;
        }
        let tool_events_cont = format!("{WORK_DIR_IN_CONTAINER}/tool-events.jsonl");
        mounts.push(Mount::rw(tool_events_host, tool_events_cont));

        // Mount the CURRENT stage's file rw, and only that file. Sibling
        // stage files remain ro under the base mount, blocking
        // cross-stage phantom-merge edits like
        //   sed -i 's/merged: false/merged: true/' .work/stages/02-other-stage.md
        // `loom stage complete` continues to work because it only writes
        // the agent's own stage file.
        //
        // Stage files are named with a depth prefix: `<NN>-<id>.md`. The
        // orchestrator writes the file during init, so it always exists
        // by spawn time.
        let stages_dir = self.work_dir.join("stages");
        if stages_dir.exists() {
            let entries = std::fs::read_dir(&stages_dir).ok();
            if let Some(entries) = entries {
                let suffix = format!("-{}.md", stage.id);
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.ends_with(&suffix) {
                        let host_path = entry.path();
                        let cont_path = format!("{WORK_DIR_IN_CONTAINER}/stages/{}", name_str);
                        mounts.push(Mount::rw(host_path, cont_path));
                        break;
                    }
                }
            }
        }

        // Layer 3b: ro overlays over sensitive `.work/` files for stages
        // that have the FULL rw `/repo` (knowledge/merge/baseconflict).
        // Without these, an agent could:
        //   - rewrite `[project_execution.container].forward_credentials`
        //     in `.work/config.toml` to escalate the next spawn's mounted
        //     credentials beyond the operator's intent
        //   - overwrite `.work/daemon.token` to deny daemon connections
        //   - clobber `.work/orchestrator.lock` / `.pid` to confuse status
        // Standard / IntegrationVerify don't need these because they get
        // the ro `/repo` base which already protects these paths.
        if needs_full_rw_repo {
            for sensitive in [
                "config.toml",
                "daemon.token",
                "orchestrator.lock",
                "orchestrator.pid",
                "orchestrator.log",
            ] {
                let host_path = self.work_dir.join(sensitive);
                if host_path.exists() {
                    let cont_path = format!("{WORK_DIR_IN_CONTAINER}/{sensitive}");
                    mounts.push(Mount::ro(host_path, cont_path));
                }
            }
        }

        // Layer 4: settings.local.json ro overlay. Pinned read-only AFTER
        // the worktree rw mount (order matters — the later mount shadows
        // anything underneath) so the agent cannot edit its own permission
        // grants mid-session.
        //
        // Source-of-truth selection:
        //   - Worktree-backed stages (Standard, IntegrationVerify,
        //     KnowledgeDistill): mount from the worktree's own
        //     `<repo>/.worktrees/<id>/.claude/settings.local.json`, written
        //     by `setup_hooks_for_worktree` at worktree creation time.
        //   - Non-worktree container sessions (Knowledge, Merge, BaseConflict):
        //     mount from the loom-owned per-session overlay at
        //     `<work_dir>/container-settings/<session-id>.local.json`. This
        //     keeps the host's `<repo>/.claude/settings.local.json` untouched
        //     so the operator's host Claude sessions remain functional.
        let uses_worktree = matches!(session_type, SessionType::Stage)
            && matches!(
                stage.stage_type,
                StageType::Standard | StageType::IntegrationVerify | StageType::KnowledgeDistill
            );
        let (settings_local_host, settings_local_cont) = if uses_worktree {
            (
                host_repo_root
                    .join(".worktrees")
                    .join(&stage.id)
                    .join(".claude/settings.local.json"),
                format!(
                    "{REPO_MOUNT}/.worktrees/{}/.claude/settings.local.json",
                    stage.id
                ),
            )
        } else {
            (
                crate::hooks::container_main_settings_path(&self.work_dir, session_id),
                format!("{REPO_MOUNT}/.claude/settings.local.json"),
            )
        };
        if settings_local_host.exists() {
            mounts.push(Mount::ro(settings_local_host, settings_local_cont));
        }

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

        // Per-stage bare mirror (read-only). Stage 4 (isolated git
        // architecture) replaces the `.git` rw bind-mount with a
        // container-private clone made from this RO mirror at container
        // entrypoint time. The agent commits to /repo (in the
        // container's writable image layer) and the daemon extracts a
        // bundle at completion time. The mirror itself can never be
        // mutated from inside the container — RO + per-stage scope
        // closes Codex blocker B1 (container-escape via git hooks).
        //
        // The mirror is created by `init_bare_mirror` BEFORE spawn
        // (see git_bridge::init_bare_mirror); if it is not present
        // here we skip the mount silently — the spawn flow that
        // populates it has not run yet, which is the case in
        // existing unit tests and during back-compat operation while
        // legacy in-container completion is still supported.
        let mirror_src = git_bridge::bare_mirror_path(&self.work_dir, &stage.id);
        if mirror_src.exists() {
            mounts.push(Mount::ro(mirror_src, MIRROR_MOUNT));
        }

        // Loom binary (read-only) — the agent inside the container needs
        // `loom` on PATH to call back into the orchestrator
        // (`loom knowledge update`, `loom memory note`, `loom stage
        // complete`, etc.). The container image does NOT ship loom (it
        // would bloat every image and tightly couple image build to loom
        // releases). Instead we bind-mount the host's running loom
        // binary at `/usr/local/bin/loom` so it is found by the default
        // PATH lookup inside the container.
        //
        // Resolved via `std::env::current_exe()` — guaranteed to be the
        // exact binary that spawned this orchestrator, so the agent
        // always sees a version-consistent CLI. Falls back silently
        // (without mounting) only when `current_exe()` returns an error,
        // which on Linux indicates a kernel that doesn't expose
        // `/proc/self/exe`; the agent will see the existing
        // `loom: command not found` message and the operator can
        // diagnose from there.
        if let Ok(loom_bin) = std::env::current_exe() {
            if loom_bin.exists() {
                mounts.push(Mount::ro(loom_bin, "/usr/local/bin/loom"));
            }
        }

        // Defense-in-depth: assert that no mount source path references the
        // relocated admin.token host path. This is impossible by construction
        // (we never mount $XDG_RUNTIME_DIR), but a future refactor that
        // introduces forward_credentials = "admin" or similar could regress
        // this. Catch it at debug-build time.
        #[cfg(debug_assertions)]
        {
            let admin_path = dirs::runtime_dir()
                .unwrap_or_else(|| dirs::data_dir().expect("no runtime/data dir"))
                .join("loom")
                .join("admin.token");
            for m in &mounts {
                debug_assert!(
                    m.source != admin_path,
                    "container mount must never expose admin.token (source: {})",
                    m.source.display()
                );
            }
        }

        Ok(mounts)
    }

    fn build_env_for_session(
        &self,
        stage_id: &str,
        session_id: &str,
        workspace_in_container: &Path,
    ) -> Vec<(String, String)> {
        let mut env = vec![
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
            // Container-mode flag — `loom stage complete` inside the
            // container reads this to detect that completion should
            // be delegated to the host daemon over the Unix socket
            // (Request::CompleteStageContainer) rather than mutating
            // host stage state directly. This is the architectural
            // signal for host-authoritative completion (Codex B2).
            ("LOOM_BACKEND".to_string(), "container".to_string()),
            (
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                "1".to_string(),
            ),
            // git refuses to ask interactively for credentials inside the
            // container by routing askpass at /bin/false.
            ("GIT_ASKPASS".to_string(), "/bin/false".to_string()),
        ];

        // Inject operator git identity when both name and email are present.
        // Partial identity (only one set) is intentionally skipped — git would
        // use inconsistent author vs committer values, which is harder to debug
        // than "git fell back to its own defaults".
        if let (Some(name), Some(email)) = (&self.git_user_name, &self.git_user_email) {
            env.push(("GIT_AUTHOR_NAME".to_string(), name.clone()));
            env.push(("GIT_AUTHOR_EMAIL".to_string(), email.clone()));
            env.push(("GIT_COMMITTER_NAME".to_string(), name.clone()));
            env.push(("GIT_COMMITTER_EMAIL".to_string(), email.clone()));
        }

        env
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

        // Cheap retry-collision defense: remove any leftover container with
        // this name so a subsequent `run` doesn't hit "container name already
        // in use". Errors are silently swallowed — `rm -f` exits 0 even when
        // no container exists. The canonical cleanup path (with log capture)
        // is `kill_session`.
        preemptive_remove_existing(self.runtime, &container_name);

        // Materialise the per-stage allowlist file before launch so the
        // in-container firewall script has a populated policy to read
        // (mounted RO at /etc/loom/network/allowed_domains.txt).
        network::write_allowlist(&self.work_dir, &self.network)
            .with_context(|| format!("Failed to write network allowlist for stage {}", stage.id))?;

        let network = self.network_name(&stage.id);
        network::ensure_network(&self.runtime, &stage.id).with_context(|| {
            format!("Failed to create container network for stage {}", stage.id)
        })?;

        // Build the claude command (escaped). Two differences from the
        // native backend's invocation:
        //
        // 1. PATH-resolved `claude` (no `find_claude_path()`). The
        //    Dockerfile installs Claude Code via `npm install -g`; the
        //    binary lives on the container's PATH at `/usr/local/bin/claude`.
        //    Baking the host's path into the wrapper produces an
        //    `exec: not found` failure inside the container.
        //
        // 2. `--print --output-format stream-json --verbose`.
        //    Claude Code's default mode is an Ink/React TUI that writes
        //    ANSI escape sequences to a real terminal — but the container
        //    has no host terminal attached. Without `--print`, claude
        //    either hangs detecting no-TTY or emits redraw sequences
        //    that `podman logs` cannot render usefully.
        //
        //    `--print` alone (text format) only emits the FINAL response
        //    after claude finishes the entire stage — for a long-running
        //    stage like knowledge-bootstrap this means silent
        //    `podman logs` for minutes at a time and no way to tell
        //    whether the agent is making progress or hung.
        //
        //    `--output-format stream-json` emits one JSON event per
        //    tool call, message chunk, and lifecycle event in realtime.
        //    `loom container logs -f` can stream these events; future
        //    `loom container logs --pretty` can format for humans.
        //    `--verbose` keeps the per-event payloads detailed.
        let escaped_prompt = escape(Cow::Borrowed(signal_prompt.as_str()));
        let claude_cmd = format!(
            "claude --print --output-format stream-json --verbose --model {} --effort {} {escaped_prompt}",
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
        let mounts = self.build_mounts(stage, session.session_type, &session.id)?;
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

        let wait_timeout = Duration::from_secs(10);
        if let Err(wait_err) = wait_until_running(self.runtime, &container_name, wait_timeout) {
            // Container failed to reach Running. Capture logs before the
            // forced removal so investigators can read the entrypoint /
            // firewall stderr that explains the failure.
            let tail = logs_capture::capture_logs(
                self.runtime,
                &container_name,
                Some(logs_capture::DEFAULT_TAIL),
            )
            .unwrap_or_default();
            let path =
                logs_capture::persist_log(&self.work_dir, &stage.id, &session.id, &tail).ok();
            let first_lines: String = tail.lines().take(20).collect::<Vec<_>>().join("\n");
            let _ = Command::new(self.runtime.binary())
                .args(["rm", "-f", &container_name])
                .status();
            bail!(
                "Container `{}` did not reach Running state within {} seconds \
                 (underlying error: {}). Captured logs saved to {:?}. Tail: {}",
                container_name,
                wait_timeout.as_secs(),
                wait_err,
                path,
                first_lines
            );
        }

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

        // Optional: attach a host terminal that streams container logs.
        if !no_attach {
            // Best-effort: failure to attach must not roll back the
            // container spawn.
            if let Ok(terminal) = detect_terminal() {
                // `logs -f` follows the container's stdout/stderr, which
                // covers entrypoint + firewall + wrapper + claude output
                // (not just the post-exec session log file).
                let exec_cmd = format!(
                    "{rt} logs -f {name}",
                    rt = self.runtime.binary(),
                    name = escape(Cow::Borrowed(&container_name)),
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
            stage, session, workspace, prompt, &title, true, &model, &effort,
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
            stage, session, workspace, prompt, &title, true, "opus[1m]", "xhigh",
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
            stage, session, workspace, prompt, &title, true, "opus[1m]", "xhigh",
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
            stage, session, workspace, prompt, &title, true, &model, &effort,
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

        // Capture and persist the container's log tail BEFORE removal so
        // crash investigators (and `loom status`) can read it after the
        // container is gone. Best-effort: a capture failure must never
        // block container removal.
        {
            let tail =
                logs_capture::capture_logs(self.runtime, name, Some(logs_capture::DEFAULT_TAIL))
                    .unwrap_or_default();
            let _ = logs_capture::persist_log(
                &self.work_dir,
                session.stage_id.as_deref().unwrap_or(&session.id),
                &session.id,
                &tail,
            );
        }

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

/// Remove any container with the given name before spawning.
///
/// This is the cheap retry-collision defence: if a prior attempt left an
/// orphan container behind the subsequent `run` would fail with
/// "container name already in use". `rm -f` is idempotent — it exits 0
/// even when no matching container exists, so errors are always ignored.
/// The canonical cleanup path (with log capture) is `kill_session`.
pub(crate) fn preemptive_remove_existing(runtime: Runtime, name: &str) {
    let _ = Command::new(runtime.binary())
        .args(["rm", "-f", name])
        .output();
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
    use tempfile::TempDir;

    /// Verify that `preemptive_remove_existing` does not panic and silently
    /// swallows errors when the container does not exist. This exercises the
    /// function against the real runtime binary (if present) or a missing
    /// binary — either way the function must return without panicking.
    #[test]
    fn preemptive_remove_existing_is_infallible() {
        // Use Docker as a representative runtime. The container name is
        // deliberately invalid/nonexistent so `rm -f` is a fast no-op.
        preemptive_remove_existing(Runtime::Docker, "loom-nonexistent-test-container-xyz");
        // If we reach here the function swallowed the error correctly.
    }

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

    /// Helper: spin up a fake repo root with a `.work/` subdirectory and
    /// return both paths plus a `ContainerBackend` ready for `build_mounts`.
    fn fixture_backend(
        stage_type: StageType,
        stage_id: &str,
    ) -> (TempDir, PathBuf, ContainerBackend, Stage) {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path().to_path_buf();
        let work_dir = repo_root.join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();
        // host_repo_root() canonicalizes work_dir then takes parent.
        let backend = ContainerBackend {
            runtime: Runtime::Docker,
            work_dir,
            image_ref: "sha256:test".to_string(),
            forward_credentials: vec![],
            git_user_name: None,
            git_user_email: None,
            network: NetworkConfig::default(),
        };
        let stage = Stage {
            id: stage_id.to_string(),
            stage_type,
            ..Stage::default()
        };
        (tmp, repo_root, backend, stage)
    }

    #[test]
    fn build_mounts_standard_stage_has_ro_repo_and_rw_worktree() {
        let (_tmp, repo_root, backend, stage) = fixture_backend(StageType::Standard, "stage-alpha");
        let mounts = backend
            .build_mounts(&stage, SessionType::Stage, "sess-1")
            .unwrap();

        // Layer 1: /repo is ro and points at host repo root.
        assert!(mounts[0].read_only, "base /repo must be read-only");
        assert_eq!(mounts[0].target, PathBuf::from("/repo"));
        assert_eq!(
            mounts[0].source.canonicalize().unwrap(),
            repo_root.canonicalize().unwrap()
        );

        // Layer 2: rw worktree overlay.
        let wt_target = PathBuf::from("/repo/.worktrees/stage-alpha");
        assert!(
            mounts.iter().any(|m| m.target == wt_target && !m.read_only),
            "expected rw mount on {}",
            wt_target.display()
        );

        // Layer 3: all four required .work/ rw layers are present.
        let work_subs = ["sessions", "memory", "handoffs", "crashes"];
        for sub in work_subs {
            let target = PathBuf::from(format!("/repo/.work/{sub}"));
            assert!(
                mounts.iter().any(|m| m.target == target && !m.read_only),
                "expected rw mount on /repo/.work/{sub}"
            );
        }
    }

    #[test]
    fn build_mounts_knowledge_stage_has_full_rw_repo() {
        // Knowledge stages need full rw /repo (not the narrow
        // doc/loom/knowledge overlay) because they:
        //   - commit directly to main (.git/ must be writable)
        //   - call `loom stage complete` (.work/stages/ must be writable)
        //   - run in the main repo, not a worktree
        let (_tmp, _repo_root, backend, stage) =
            fixture_backend(StageType::Knowledge, "kn-bootstrap");
        let mounts = backend
            .build_mounts(&stage, SessionType::Knowledge, "sess-2")
            .unwrap();

        // /repo itself must be rw (no ro base for knowledge stages).
        assert!(
            !mounts[0].read_only,
            "Knowledge stage: /repo must be rw (full repo write access required for git commit and loom stage complete)"
        );
        assert_eq!(mounts[0].target, PathBuf::from("/repo"));

        // No worktree rw overlay — knowledge stages run in main repo.
        assert!(
            !mounts
                .iter()
                .any(|m| m.target.starts_with("/repo/.worktrees/")),
            "Knowledge stage should not get a worktree rw overlay"
        );
    }

    #[test]
    fn build_mounts_includes_settings_local_ro_overlay() {
        let (_tmp, repo_root, backend, stage) = fixture_backend(StageType::Standard, "stage-beta");
        // Pre-create the settings.local.json fixture on the worktree path.
        let settings_path = repo_root
            .join(".worktrees")
            .join(&stage.id)
            .join(".claude/settings.local.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(&settings_path, b"{}").unwrap();

        let mounts = backend
            .build_mounts(&stage, SessionType::Stage, "sess-3")
            .unwrap();
        let target = PathBuf::from("/repo/.worktrees/stage-beta/.claude/settings.local.json");

        let settings_idx = mounts
            .iter()
            .position(|m| m.target == target)
            .expect("settings.local.json mount missing");
        assert!(
            mounts[settings_idx].read_only,
            "settings.local.json must be mounted ro"
        );

        // Order check: ro settings overlay must come AFTER the worktree
        // rw mount so it shadows the file inside.
        let wt_target = PathBuf::from("/repo/.worktrees/stage-beta");
        let wt_idx = mounts
            .iter()
            .position(|m| m.target == wt_target)
            .expect("worktree rw mount missing");
        assert!(
            settings_idx > wt_idx,
            "settings.local.json ro overlay (idx {settings_idx}) must come after \
             worktree rw mount (idx {wt_idx})"
        );
    }

    #[test]
    fn build_mounts_no_rw_overlap_with_work_config_toml() {
        let (_tmp, _repo_root, backend, stage) =
            fixture_backend(StageType::Standard, "stage-gamma");
        let mounts = backend
            .build_mounts(&stage, SessionType::Stage, "sess-4")
            .unwrap();

        // No rw mount may have a source path that *is* config.toml or that
        // contains it inside the mounted subtree. Iterate mounts; assert
        // no rw source equals `.work/config.toml` or its parent (`.work/`).
        let config_toml = backend.work_dir.join("config.toml");
        for m in &mounts {
            if m.read_only {
                continue;
            }
            assert!(
                m.source != config_toml,
                "rw mount must not target .work/config.toml directly: {}",
                m.source.display()
            );
            // .work/ itself must not be mounted rw — only its enumerated
            // subdirectories. (`.work/config.toml` lives directly under
            // `.work/`, so an rw mount on `.work/` would expose it.)
            assert!(
                m.source != backend.work_dir,
                "rw mount must not cover the entire .work/ directory: {}",
                m.source.display()
            );
        }
    }

    #[test]
    fn build_mounts_includes_hook_observability_rw_paths() {
        // Claude Code hooks (session-start, session-end, pre-compact,
        // post-tool-use) write into four .work/ subdirectories and one
        // top-level append-only file. Without rw bind mounts for these
        // paths, every container session silently loses observability
        // events (events.jsonl, heartbeat markers, compaction recovery
        // markers, tool-events.jsonl) — and the session-end hook in
        // particular fails noisily with EROFS, getting misread by the
        // orchestrator as a session crash that triggers retry.
        let (_tmp, _repo_root, backend, stage) =
            fixture_backend(StageType::Standard, "stage-hooks");
        let mounts = backend
            .build_mounts(&stage, SessionType::Stage, "sess-hooks")
            .unwrap();

        for sub in [
            "hooks",
            "heartbeat",
            "compaction-pending",
            "compaction-recovery",
        ] {
            let target = PathBuf::from(format!("/repo/.work/{sub}"));
            let m = mounts
                .iter()
                .find(|m| m.target == target)
                .unwrap_or_else(|| panic!("expected rw mount on /repo/.work/{sub}"));
            assert!(!m.read_only, "/repo/.work/{sub} must be mounted rw");
            assert!(
                m.source.is_dir(),
                "rw mount source for /repo/.work/{sub} must be a real directory: {}",
                m.source.display()
            );
        }

        let tool_events_target = PathBuf::from("/repo/.work/tool-events.jsonl");
        let tool_events = mounts
            .iter()
            .find(|m| m.target == tool_events_target)
            .expect("expected rw mount on /repo/.work/tool-events.jsonl");
        assert!(
            !tool_events.read_only,
            "/repo/.work/tool-events.jsonl must be mounted rw"
        );
        assert!(
            tool_events.source.is_file(),
            "rw mount source for tool-events.jsonl must be a real file (pre-created): {}",
            tool_events.source.display()
        );
        // Pre-creating must not clobber existing content: a second
        // build_mounts call against the same backend leaves the host
        // file alone.
        std::fs::write(&tool_events.source, b"{\"old\":\"event\"}\n").unwrap();
        let _ = backend
            .build_mounts(&stage, SessionType::Stage, "sess-hooks-2")
            .unwrap();
        assert_eq!(
            std::fs::read(&tool_events.source).unwrap(),
            b"{\"old\":\"event\"}\n",
            "tool-events.jsonl pre-create must not truncate an existing file"
        );
    }

    #[test]
    fn build_mounts_knowledge_session_uses_per_session_settings_overlay() {
        // Container-backed knowledge stages must mount the loom-owned
        // per-session overlay at `<work_dir>/container-settings/<session>.local.json`
        // rather than the host's `<repo>/.claude/settings.local.json`. This
        // keeps the operator's host Claude settings untouched.
        let (_tmp, repo_root, backend, stage) =
            fixture_backend(StageType::Knowledge, "kn-bootstrap");

        // Pre-create the per-session overlay AND a host settings.local.json
        // so we can verify the mount sources the per-session file.
        let session_id = "session-abc";
        let overlay = backend
            .work_dir
            .join("container-settings")
            .join(format!("{session_id}.local.json"));
        std::fs::create_dir_all(overlay.parent().unwrap()).unwrap();
        std::fs::write(&overlay, b"{}").unwrap();

        let host_settings = repo_root.join(".claude/settings.local.json");
        std::fs::create_dir_all(host_settings.parent().unwrap()).unwrap();
        std::fs::write(&host_settings, b"{\"host\": true}").unwrap();

        let mounts = backend
            .build_mounts(&stage, SessionType::Knowledge, session_id)
            .unwrap();

        let target = PathBuf::from("/repo/.claude/settings.local.json");
        let m = mounts
            .iter()
            .find(|m| m.target == target)
            .expect("settings.local.json mount missing for knowledge session");
        assert!(m.read_only, "settings.local.json mount must be ro");
        assert_eq!(
            m.source.canonicalize().unwrap(),
            overlay.canonicalize().unwrap(),
            "knowledge session must mount the per-session overlay, not the host file"
        );
        assert_ne!(
            m.source.canonicalize().unwrap(),
            host_settings.canonicalize().unwrap(),
            "knowledge session must NOT mount the host's <repo>/.claude/settings.local.json"
        );
    }

    #[test]
    fn build_mounts_merge_session_has_rw_repo() {
        let (_tmp, _repo_root, backend, stage) =
            fixture_backend(StageType::Standard, "stage-delta");

        for st in [SessionType::Merge, SessionType::BaseConflict] {
            let mounts = backend.build_mounts(&stage, st, "sess-merge").unwrap();
            assert!(
                mounts[0].target.as_path() == Path::new("/repo") && !mounts[0].read_only,
                "{st:?} session should mount /repo rw (documented exception)"
            );
            // No narrow rw overlay should appear because /repo is already
            // rw — but the .work/ subtrees still get explicit rw mounts.
            assert!(
                mounts.iter().any(
                    |m| m.target.as_path() == Path::new("/repo/.work/sessions") && !m.read_only
                ),
                "{st:?} session should still mount .work/sessions rw"
            );
        }
    }

    #[test]
    fn build_env_injects_git_identity_when_both_set() {
        let tmp = TempDir::new().unwrap();
        let work_dir = tmp.path().join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();
        let backend = ContainerBackend {
            runtime: Runtime::Docker,
            work_dir,
            image_ref: "sha256:test".to_string(),
            forward_credentials: vec![],
            git_user_name: Some("Bob Builder".to_string()),
            git_user_email: Some("bob@example.com".to_string()),
            network: NetworkConfig::default(),
        };
        let workspace = Path::new("/repo/.worktrees/stage-x");
        let env = backend.build_env_for_session("stage-x", "session-1", workspace);
        let env_map: std::collections::HashMap<_, _> =
            env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert_eq!(env_map.get("GIT_AUTHOR_NAME"), Some(&"Bob Builder"));
        assert_eq!(env_map.get("GIT_AUTHOR_EMAIL"), Some(&"bob@example.com"));
        assert_eq!(env_map.get("GIT_COMMITTER_NAME"), Some(&"Bob Builder"));
        assert_eq!(env_map.get("GIT_COMMITTER_EMAIL"), Some(&"bob@example.com"));
    }

    #[test]
    fn container_mount_does_not_include_admin_token_host_path() {
        // Even with all stage types and session types, no mount source must
        // reference the relocated admin.token path. This is the structural
        // backstop for the admin capability gate: containers cannot read the
        // admin token because it is never mounted in.
        let (_tmp, _repo_root, backend, stage) =
            fixture_backend(StageType::Standard, "stage-no-admin-mount");
        let admin_path = dirs::runtime_dir()
            .unwrap_or_else(|| dirs::data_dir().expect("no runtime/data dir"))
            .join("loom")
            .join("admin.token");
        for session_type in [
            SessionType::Stage,
            SessionType::Knowledge,
            SessionType::Merge,
            SessionType::BaseConflict,
        ] {
            let mounts = backend.build_mounts(&stage, session_type, "sess-x").unwrap();
            for m in &mounts {
                assert_ne!(
                    m.source, admin_path,
                    "{session_type:?} session must NOT mount admin.token (source: {})",
                    m.source.display()
                );
            }
        }
    }

    #[test]
    fn build_env_omits_git_identity_when_either_missing() {
        let tmp = TempDir::new().unwrap();
        let work_dir = tmp.path().join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();

        for (name, email) in [
            (None, None),
            (Some("Alice".to_string()), None),
            (None, Some("alice@example.com".to_string())),
        ] {
            let backend = ContainerBackend {
                runtime: Runtime::Docker,
                work_dir: work_dir.clone(),
                image_ref: "sha256:test".to_string(),
                forward_credentials: vec![],
                git_user_name: name,
                git_user_email: email,
                network: NetworkConfig::default(),
            };
            let workspace = Path::new("/repo/.worktrees/stage-x");
            let env = backend.build_env_for_session("stage-x", "session-1", workspace);
            let keys: Vec<_> = env.iter().map(|(k, _)| k.as_str()).collect();
            // Partial identity must suppress ALL four GIT_* vars, not just
            // the *_NAME pair. A leaked *_EMAIL would produce a half-set
            // identity that git either rejects or accepts inconsistently.
            for key in [
                "GIT_AUTHOR_NAME",
                "GIT_AUTHOR_EMAIL",
                "GIT_COMMITTER_NAME",
                "GIT_COMMITTER_EMAIL",
            ] {
                assert!(
                    !keys.contains(&key),
                    "should not set {key} when identity incomplete",
                );
            }
        }
    }
}
