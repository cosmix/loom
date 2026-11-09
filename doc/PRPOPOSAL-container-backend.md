# Loom: Container Backend & Auto-Mode Permission Plumbing

**Status:** Draft implementation specification
**Target:** `cosmix/loom`
**Audience:** Implementer (human or Claude Code session)

---

## 1. Motivation

Loom currently spawns Claude Code sessions on the host via `NativeBackend`, with `permissions.defaultMode` hard-coded to `acceptEdits` in worktree settings. This forces a tradeoff:

- **acceptEdits + native** is what loom does now. Safe-ish, but Claude Code's sandbox-exec layer on macOS and the in-CLI permissions system have a track record of being inconsistent. Stages still pause for permission prompts on commands the classifier hasn't pre-approved, defeating swarm parallelism.
- **bypassPermissions + native** removes prompts but is unsafe on a developer host. Not an option.
- **auto + native** is the middle ground (classifier-reviewed actions), but loom never wires it.

What we want: every standard implementation stage runs unattended, the host stays unreachable from a misbehaving session, and the existing worktree/permission/hooks plumbing keeps working untouched.

The fix has two independent pieces:

1. **Permission-mode plumbing.** Replace the hard-coded `acceptEdits` with a per-stage-type resolved mode (`auto` for Standard/IntegrationVerify, `acceptEdits` for Knowledge/KnowledgeDistill). Adds a `permission_mode` knob to the plan and stage schemas. ~200 lines.
2. **Container backend.** New `BackendType::Container` with a `ContainerBackend` impl of `TerminalBackend`. Each stage runs Claude Code in a per-stage container with the worktree bind-mounted, network egress firewalled, and host credentials excluded. Unlocks `bypassPermissions` safely. ~1500-2000 lines including image build and tests.

Phase 1 ships value on its own. Phase 2 builds on it.

---

## 2. Goals and non-goals

### Goals

- Standard stages run unattended under auto mode by default, no plan changes required.
- Plan authors can override `permission_mode` per stage and per plan.
- `loom init <plan> --backend container` is the single user-facing entry point for container mode. It detects the runtime, builds the image (if not already cached for the project's language fingerprint), pins the digest, and writes the backend choice to `.work/config.toml`. No separate setup command, no per-project Dockerfile to maintain.
- `bypassPermissions` becomes a usable mode, but only when the active backend is `Container`. Native + bypass refuses to start.
- Existing hooks (`worktree-isolation.sh`, `worktree-file-guard.sh`, `commit-guard.sh`, etc.) keep working unchanged inside the container.
- Cross-platform: Linux (Podman preferred, Docker fallback) and macOS (Apple Container preferred, Docker Desktop fallback).

### Non-goals

- Firecracker microVMs. Apple Container already gives per-container VM isolation on macOS; Podman with userns is enough on Linux. Add Firecracker later if needed.
- Windows host support beyond what loom already does (best-effort WSL).
- Replacing the existing OS-level Claude Code sandbox. Containers are an additional layer, not a replacement. Filesystem deny rules still apply inside the container.
- Remote container hosts (Docker contexts pointing at remote daemons). Local only.
- A managed image registry. Images are built locally and cached per machine, keyed by detected-language fingerprint.

---

## 3. Architecture

### 3.1 Current backend trait (unchanged)

`loom/src/orchestrator/terminal/mod.rs` defines:

```rust
pub trait TerminalBackend: Send + Sync {
    fn spawn_session(&self, stage: &Stage, worktree: &Worktree, session: Session, signal_path: &Path) -> Result<Session>;
    fn spawn_merge_session(&self, stage: &Stage, session: Session, signal_path: &Path, repo_root: &Path) -> Result<Session>;
    fn spawn_base_conflict_session(&self, stage: &Stage, session: Session, signal_path: &Path, repo_root: &Path) -> Result<Session>;
    fn spawn_knowledge_session(&self, stage: &Stage, session: Session, signal_path: &Path, repo_root: &Path) -> Result<Session>;
    fn kill_session(&self, session: &Session) -> Result<()>;
    fn is_session_alive(&self, session: &Session) -> Result<bool>;
    fn backend_type(&self) -> BackendType;
}
```

`ContainerBackend` implements every method. The contract is identical from the orchestrator's perspective.

### 3.2 Container topology

One container per stage. Naming mirrors the existing window-title convention so kill-by-name still works:

| Session kind        | Container name                  | Working dir  |
| ------------------- | ------------------------------- | ------------ |
| Standard stage      | `loom-{stage_id}`               | `/workspace` |
| Merge resolution    | `loom-merge-{stage_id}`         | `/repo`      |
| Base conflict       | `loom-base-conflict-{stage_id}` | `/repo`      |
| Knowledge gathering | `loom-knowledge-{stage_id}`     | `/repo`      |

Bind mounts:

```
host                                    container
─────                                   ─────────
{worktree_path}                  rw →   /workspace
{repo_root}/.work                rw →   /work
~/.claude/.credentials.json      ro →   /home/loom/.claude/.credentials.json
{repo_root} (merge/knowledge)    rw →   /repo
```

Per-container bridge network `loom-net-{stage_id}` so sibling stages can't reach each other even if egress rules drift.

The wrapper script (already generated by `pid_tracking::create_wrapper_script`) is written into `.work/wrappers/{stage_id}.sh` and becomes the container's command. Inside the container it does the same `cd $WORKSPACE; export LOOM_*; exec claude ...` it does today. The host-side terminal window opens a `docker exec` (or `podman exec` / `container exec`) attached to that container so the user can watch.

### 3.3 Permission-mode resolution

Resolution order (first match wins):

1. Stage-level `sandbox.permission_mode` (override).
2. Plan-level `sandbox.permission_mode`.
3. Stage-type default (see table below).

Stage-type defaults:

| Stage type          | Default mode  | Rationale                                              |
| ------------------- | ------------- | ------------------------------------------------------ |
| `Standard`          | `auto`        | Classifier review fits unattended implementation work. |
| `IntegrationVerify` | `auto`        | Same; verification often runs commands.                |
| `Knowledge`         | `acceptEdits` | Mostly read; writes go via `loom knowledge update`.    |
| `KnowledgeDistill`  | `acceptEdits` | Bounded curation work.                                 |

`bypassPermissions` is permitted as an explicit override only when the resolved backend is `Container`. The resolver fails loudly otherwise.

### 3.4 Backend selection

Backend is set per project at `loom init` time via the `--backend` flag and persisted to `.work/config.toml`. There is no global default; the flag is the only opt-in.

Resolution at run time (highest first):

1. Stage-level `execution.backend` in the plan (per-stage opt-out, see below).
2. `.work/config.toml` `[execution] backend = ...` (set by `loom init --backend`).
3. Default: `native`.

Stage-level overrides can only **narrow**, not widen. A container-mode project may have stages opt out to native (a stage that needs host tooling). A native-mode project may **not** have a stage demand container — that would require a runtime and an image that the project hasn't been initialised for. `loom init` validates this and fails with an actionable error if a plan demands container mode but the project is native:

```
error: stage 'integration-verify' requires execution.backend=container,
       but project backend is native.

Re-initialise with: loom init <plan> --backend container
```

There is no plan-level `execution.backend` field. Plan-level `execution.container` configuration (runtime preference, credential forwarding) is honoured, but does not switch the backend on. Switching is always a user-driven action via the flag.

Switching an existing project: `loom init <plan> --backend container` (or `--backend native`). Re-running `loom init` without `--backend` preserves the current setting.

---

## 4. Schema additions

All additions are backwards-compatible (every new field is `Option<T>` or has a default).

### 4.1 `loom/src/plan/schema/types.rs`

Add the enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Auto,
    Plan,
    BypassPermissions,
}

impl PermissionMode {
    pub fn as_settings_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }
}
```

Add to `SandboxConfig`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub permission_mode: Option<PermissionMode>,
```

Add to `StageSandboxConfig`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub permission_mode: Option<PermissionMode>,
```

### 4.2 New execution config

New file `loom/src/plan/schema/execution.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Plan-level execution configuration.
///
/// Note: `backend` selection is NOT a plan-level field. It is set per
/// project via `loom init --backend` and stored in `.work/config.toml`.
/// This struct only carries options that apply when the user has opted
/// into container mode for this project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerConfig>,
}

/// Backend selection enum.
///
/// This is the single source of truth for backend identity, used by
/// the plan schema, the orchestrator, and `.work/config.toml` serialisation.
/// The existing `BackendType` enum in `orchestrator/terminal/mod.rs` is
/// REPLACED by this — the orchestrator imports it from here. There is
/// no parallel `BackendKind` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendType {
    #[default]
    Native,
    Container,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// docker | podman | container (Apple) | auto
    #[serde(default = "default_runtime")]
    pub runtime: String,

    /// Pinned image digest, e.g. "sha256:abcd..."
    /// Set by `loom init --backend container`; do not edit by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,

    /// Host credentials to forward into the container.
    /// Allowed values: "github" | "gitlab" | "ssh-agent"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward_credentials: Vec<String>,

    /// Extra bind mounts (host:container[:ro])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<String>,
}

fn default_runtime() -> String { "auto".to_string() }
```

In `orchestrator/terminal/mod.rs`, **remove** the existing local `BackendType` enum entirely and import the canonical one:

```rust
pub use crate::plan::schema::execution::BackendType;
```

The existing `Display` and `FromStr` impls in `orchestrator/terminal/mod.rs` move to `plan/schema/execution.rs` to keep impl blocks co-located with the type.

Add to plan top-level (probably `Plan` struct in `plan/schema/mod.rs`):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub execution: Option<ExecutionConfig>,
```

Add to per-stage schema (alongside `sandbox`):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub execution: Option<StageExecutionConfig>,
```

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageExecutionConfig {
    /// Per-stage backend override. Can only narrow (container project →
    /// native stage). Cannot widen; a native project rejects stages that
    /// require container at `loom init` time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendType>,
}
```

### 4.3 `MergedSandboxConfig` extension

In `loom/src/sandbox/config.rs`:

```rust
pub struct MergedSandboxConfig {
    pub enabled: bool,
    pub auto_allow: bool,
    pub allow_unsandboxed_escape: bool,
    pub excluded_commands: Vec<String>,
    pub filesystem: FilesystemConfig,
    pub network: NetworkConfig,
    pub linux: LinuxConfig,
    pub permission_mode: PermissionMode, // NEW: resolved, never None
}
```

Resolution logic:

```rust
pub fn merge_config(
    plan_config: &SandboxConfig,
    stage_config: &StageSandboxConfig,
    stage_type: StageType,
) -> MergedSandboxConfig {
    let permission_mode = stage_config.permission_mode
        .or(plan_config.permission_mode)
        .unwrap_or_else(|| default_mode_for(stage_type));

    MergedSandboxConfig {
        // ... existing fields ...
        permission_mode,
    }
}

fn default_mode_for(stage_type: StageType) -> PermissionMode {
    match stage_type {
        StageType::Standard | StageType::IntegrationVerify => PermissionMode::Auto,
        StageType::Knowledge | StageType::KnowledgeDistill => PermissionMode::AcceptEdits,
    }
}
```

Validation (called from `validate_paths` or a new `validate_config`):

```rust
pub fn validate_config(merged: &MergedSandboxConfig, backend: BackendType) -> Result<()> {
    if merged.permission_mode == PermissionMode::BypassPermissions
        && backend != BackendType::Container
    {
        bail!(
            "permission_mode=bypassPermissions requires execution.backend=container. \
             Refusing to start to prevent host filesystem damage."
        );
    }
    Ok(())
}
```

In Phase 1, only `BackendType::Native` exists at the orchestrator level (Phase 2 adds `Container`). `validate_config` therefore unconditionally rejects `BypassPermissions` until Phase 2 ships, which is the correct behaviour: bypass on native must never be permitted, regardless of phase.

---

## 5. Phase 1: Permission-mode plumbing (no container code)

### 5.1 Files touched

- `loom/src/plan/schema/types.rs` — add `PermissionMode`, fields on `SandboxConfig` and `StageSandboxConfig`.
- `loom/src/sandbox/config.rs` — extend `MergedSandboxConfig`, resolution logic, `default_mode_for`.
- `loom/src/sandbox/settings.rs` — read resolved mode, write to `defaultMode` in `generate_settings_json`.
- `loom/src/git/worktree/settings.rs` — replace hard-coded `"acceptEdits"` at line 338 with the resolved value. The signature of `create_worktree_settings` takes a `PermissionMode` parameter.
- `loom/src/hooks/generator.rs:42` — same replacement.

### 5.2 The hard-coded sites

There are three places that currently write `"acceptEdits"`. All three must use the resolved value from `MergedSandboxConfig`:

1. `git/worktree/settings.rs:338` — worktree creation path.
2. `hooks/generator.rs:42` — hook bootstrap path.
3. `sandbox/settings.rs::generate_settings_json` — per-stage settings file (this one currently doesn't write `defaultMode` at all; it should).

Extract a single helper:

```rust
// loom/src/sandbox/settings.rs
pub fn apply_default_mode(settings: &mut Value, mode: PermissionMode) -> Result<()> {
    let permissions = settings
        .as_object_mut()
        .and_then(|o| o.entry("permissions").or_insert_with(|| json!({})).as_object_mut())
        .ok_or_else(|| anyhow!("permissions must be a JSON object"))?;
    permissions.insert("defaultMode".to_string(), json!(mode.as_settings_value()));
    Ok(())
}
```

Call from all three sites.

### 5.3 Tests

Add to `loom/src/sandbox/config.rs`:

```rust
#[test]
fn standard_stage_defaults_to_auto() {
    let plan = SandboxConfig::default();
    let stage = StageSandboxConfig::default();
    let merged = merge_config(&plan, &stage, StageType::Standard);
    assert_eq!(merged.permission_mode, PermissionMode::Auto);
}

#[test]
fn knowledge_stage_defaults_to_accept_edits() {
    let merged = merge_config(&SandboxConfig::default(), &StageSandboxConfig::default(), StageType::Knowledge);
    assert_eq!(merged.permission_mode, PermissionMode::AcceptEdits);
}

#[test]
fn stage_overrides_plan() {
    let plan = SandboxConfig { permission_mode: Some(PermissionMode::AcceptEdits), ..Default::default() };
    let stage = StageSandboxConfig { permission_mode: Some(PermissionMode::Plan), ..Default::default() };
    let merged = merge_config(&plan, &stage, StageType::Standard);
    assert_eq!(merged.permission_mode, PermissionMode::Plan);
}

#[test]
fn bypass_rejected_on_native_backend() {
    let merged = MergedSandboxConfig {
        permission_mode: PermissionMode::BypassPermissions,
        ..test_default()
    };
    assert!(validate_config(&merged, BackendType::Native).is_err());
}

#[test]
fn bypass_accepted_on_container_backend() {
    let merged = MergedSandboxConfig {
        permission_mode: PermissionMode::BypassPermissions,
        ..test_default()
    };
    assert!(validate_config(&merged, BackendType::Container).is_ok());
}
```

Add tests for the writer in `git/worktree/settings.rs` covering all five `PermissionMode` values.

### 5.4 Acceptance criteria for Phase 1

- `cargo test -p loom` passes.
- Running `loom run` against a plan with no `permission_mode` set produces `.claude/settings.local.json` files containing `"defaultMode": "auto"` for every Standard stage.
- Setting `sandbox.permission_mode: bypassPermissions` at the plan level fails `loom init` (or `loom run`, whichever validates first) with the explanatory error from `validate_config`.
- Existing plans continue to work without modification.

---

## 6. Phase 2: Container backend skeleton

### 6.1 New module layout

```
loom/src/orchestrator/terminal/
├── mod.rs                  (modified: add BackendType::Container)
├── native/                 (existing, unchanged)
└── container/              (new)
    ├── mod.rs              ContainerBackend impl + tests
    ├── runtime.rs          Runtime detection (docker/podman/container)
    ├── lifecycle.rs        Container create/start/exec/rm helpers
    ├── network.rs          (Phase 4) firewall script generation
    └── image.rs            (Phase 3) image build, digest pinning
```

### 6.2 `BackendType` extension

`BackendType` is defined in `plan/schema/execution.rs` (introduced in Phase 1). The `Container` variant is already in the enum from Phase 1; Phase 2 only needs to wire it through `create_backend`:

```rust
// loom/src/orchestrator/terminal/mod.rs
pub use crate::plan::schema::execution::BackendType;

pub fn create_backend(backend_type: BackendType, work_dir: &Path) -> Result<Box<dyn TerminalBackend>> {
    match backend_type {
        BackendType::Native => Ok(Box::new(native::NativeBackend::new(work_dir.to_path_buf())?)),
        BackendType::Container => Ok(Box::new(container::ContainerBackend::new(work_dir.to_path_buf())?)),
    }
}
```

The existing `Display` and `FromStr` impls were moved to `plan/schema/execution.rs` in Phase 1 and are imported via the `pub use`.

### 6.3 Runtime detection

`loom/src/orchestrator/terminal/container/runtime.rs`:

```rust
use anyhow::{bail, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Docker,
    Podman,
    AppleContainer,
}

impl Runtime {
    pub fn binary(&self) -> &'static str {
        match self {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
            Runtime::AppleContainer => "container",
        }
    }
}

pub fn detect_runtime(preference: &str) -> Result<Runtime> {
    match preference {
        "docker" => find_binary("docker").map(|_| Runtime::Docker),
        "podman" => find_binary("podman").map(|_| Runtime::Podman),
        "container" => find_binary("container").map(|_| Runtime::AppleContainer),
        "auto" => detect_auto(),
        other => bail!("Unknown container runtime: {other}"),
    }
}

#[cfg(target_os = "macos")]
fn detect_auto() -> Result<Runtime> {
    if find_binary("container").is_ok() { return Ok(Runtime::AppleContainer); }
    if find_binary("podman").is_ok() { return Ok(Runtime::Podman); }
    if find_binary("docker").is_ok() { return Ok(Runtime::Docker); }
    bail!("No container runtime found. Install Apple Container, Podman, or Docker.")
}

#[cfg(target_os = "linux")]
fn detect_auto() -> Result<Runtime> {
    if find_binary("podman").is_ok() { return Ok(Runtime::Podman); }
    if find_binary("docker").is_ok() { return Ok(Runtime::Docker); }
    bail!("No container runtime found. Install Podman or Docker.")
}

fn find_binary(name: &str) -> Result<PathBuf> {
    which::which(name).map_err(|_| anyhow::anyhow!("{name} not in PATH"))
}
```

### 6.4 `ContainerBackend` skeleton

`loom/src/orchestrator/terminal/container/mod.rs`:

```rust
mod lifecycle;
mod runtime;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{BackendType, TerminalBackend};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

pub struct ContainerBackend {
    runtime: runtime::Runtime,
    work_dir: PathBuf,
    image_ref: String,
}

impl ContainerBackend {
    pub fn new(work_dir: PathBuf) -> Result<Self> {
        let runtime = runtime::detect_runtime("auto")?;
        let image_ref = lifecycle::resolve_image_ref(&work_dir)
            .context(
                "No pinned image digest found in .work/config.toml. \
                 Run `loom init <plan> --backend container` to set up container mode.",
            )?;
        Ok(Self { runtime, work_dir, image_ref })
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
        // 1. Generate the in-container wrapper script (reuse pid_tracking::create_wrapper_script,
        //    but with container-relative paths: /workspace, /work).
        // 2. Build `docker run` args.
        // 3. Spawn host terminal that runs `docker exec -it <name> /bin/bash` (so user can watch).
        // 4. Track container PID via `docker inspect -f '{{.State.Pid}}'` and write to .work/pids/.
        // 5. Mark session running.
        unimplemented!("see lifecycle::spawn_stage_container")
    }

    fn spawn_merge_session(&self, ...) -> Result<Session> { ... }
    fn spawn_base_conflict_session(&self, ...) -> Result<Session> { ... }
    fn spawn_knowledge_session(&self, ...) -> Result<Session> { ... }

    fn kill_session(&self, session: &Session) -> Result<()> {
        if let Some(stage_id) = &session.stage_id {
            let name = format!("loom-{stage_id}");
            Command::new(self.runtime.binary())
                .args(["rm", "-f", &name])
                .output()
                .context("Failed to remove container")?;
        }
        Ok(())
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        let Some(stage_id) = &session.stage_id else { return Ok(false); };
        let name = format!("loom-{stage_id}");
        let output = Command::new(self.runtime.binary())
            .args(["inspect", "-f", "{{.State.Running}}", &name])
            .output()?;
        Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true")
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Container
    }
}
```

### 6.5 The actual `docker run` invocation

`loom/src/orchestrator/terminal/container/lifecycle.rs`, the core helper:

```rust
pub fn build_run_args(
    runtime: Runtime,
    name: &str,
    image: &str,
    worktree_abs: &Path,
    work_dir_abs: &Path,
    stage_id: &str,
    session_id: &str,
    extra_mounts: &[String],
    forward_credentials: &[String],
) -> Vec<String> {
    let creds_path = dirs::home_dir()
        .map(|h| h.join(".claude/.credentials.json"))
        .expect("home dir resolvable");

    // Use --mount form (not -v) for portability with Apple Container,
    // which requires the explicit form.
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),                 // detach: orchestrator returns immediately;
                                          // attach is a separate `exec` from the host terminal
        "--rm".to_string(),
        format!("--name=loom-{stage_id}"),
        "--cap-drop=ALL".to_string(),
        "--cap-add=NET_ADMIN".to_string(),  // for in-container firewall (no-op on Apple Container, harmless)
        "--cap-add=NET_RAW".to_string(),
        format!("--network=loom-net-{stage_id}"),

        // Bind mounts (--mount is portable across Docker, Podman, Apple Container)
        format!("--mount=type=bind,source={},target=/workspace", worktree_abs.display()),
        format!("--mount=type=bind,source={},target=/work", work_dir_abs.display()),
        format!(
            "--mount=type=bind,source={},target=/home/loom/.claude/.credentials.json,readonly",
            creds_path.display()
        ),

        // Map host UID/GID into the container so bind-mounted files are
        // readable. Without this, a host user with UID != 1000 cannot
        // read their own credentials file inside the container.
        // On Podman, --userns=keep-id maps the invoking user to the
        // container user. On Docker, --user $(id -u):$(id -g) does the
        // job. The runtime layer (Section 6.3) abstracts this.
        runtime.user_args(),  // returns &[&str]; flatten into args below

        // Loom env vars
        format!("-e=LOOM_SESSION_ID={session_id}"),
        format!("-e=LOOM_STAGE_ID={stage_id}"),
        "-e=LOOM_WORK_DIR=/work".to_string(),
        "-e=LOOM_WORKTREE_PATH=/workspace".to_string(),
        "-e=CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1".to_string(),

        // Strip host credentials unless explicitly forwarded
        "-e=SSH_AUTH_SOCK=".to_string(),
        "-e=GH_TOKEN=".to_string(),
        "-e=GITHUB_TOKEN=".to_string(),
        "-e=GIT_ASKPASS=/bin/false".to_string(),
    ];

    if forward_credentials.iter().any(|c| c == "github") {
        if let Ok(token) = std::env::var("GH_TOKEN").or_else(|_| std::env::var("GITHUB_TOKEN")) {
            // Replace the empty -e=GH_TOKEN= entry with a populated one
            args.retain(|a| a != "-e=GH_TOKEN=" && a != "-e=GITHUB_TOKEN=");
            args.push(format!("-e=GH_TOKEN={token}"));
        }
    }

    for mount in extra_mounts {
        args.push(format!("--mount={mount}"));
    }

    args.push(image.to_string());
    args.push(format!("/work/wrappers/{stage_id}.sh"));   // command inside container
    args
}
```

The container starts detached. The orchestrator records the container's host-side PID (Docker/Podman) and proceeds. The host terminal window is then opened separately to attach for the user's view (Section 6.7).

`Runtime::user_args` returns the runtime-appropriate UID/GID mapping flag set:

```rust
impl Runtime {
    pub fn user_args(&self) -> Vec<String> {
        match self {
            // Podman: rootless mode + keep-id maps host UID to the
            // container's "loom" user UID 1000. Files written inside the
            // container appear with the host user's UID on the host.
            Runtime::Podman => vec!["--userns=keep-id".to_string()],
            // Docker: explicit UID:GID mapping. The image's "loom" user
            // is created at UID 1000; if host UID differs, the container
            // process runs as host UID but inside a container that
            // doesn't have a passwd entry for that UID. This is fine for
            // bind-mount permissions; ${HOME} resolution still works
            // because we bind-mount credentials at a fixed path.
            Runtime::Docker => {
                let uid = unsafe { libc::getuid() };
                let gid = unsafe { libc::getgid() };
                vec![format!("--user={uid}:{gid}")]
            }
            // Apple Container: handles UID translation automatically
            // via its VM layer; no flag needed.
            Runtime::AppleContainer => vec![],
        }
    }
}
```

The Dockerfile must be UID-flexible: don't `chown` the workspace at build time (we don't know the runtime UID). Files in `/workspace` come from the bind mount, so their ownership is the host's. Files in `/home/loom` are pre-created at build time as UID 1000; under Docker with `--user`, the running process won't match this UID but doesn't need to write under `/home/loom` (Claude's writable state goes to `/workspace` or `/work`).

### 6.6 The wrapper script: container-relative paths

`pid_tracking::create_wrapper_script` currently bakes in absolute host paths via `canonicalize`. For container runs we need a parallel function (or a parameter):

```rust
pub fn create_wrapper_script_container(
    work_dir: &Path,         // host path, used for output location only
    stage_id: &str,
    session_id: &str,
    claude_cmd: &str,
    container_workdir: &Path, // /workspace or /repo (container-relative)
) -> Result<PathBuf> { ... }
```

The script content uses container paths (`/workspace`, `/work`) instead of host paths. The script file lives on the host in `.work/wrappers/`, but the in-container `LOOM_WORK_DIR=/work` bind-mount makes it visible at `/work/wrappers/{stage_id}.sh`.

### 6.7 The host-side terminal window

The container starts detached (`docker run -d`). To let the user watch what's happening inside, spawn a host terminal that attaches via `exec`:

```rust
let attach_cmd = format!(
    "{} exec -it loom-{} /bin/bash -lc 'tail -f /work/sessions/{}.log'",
    self.runtime.binary(),
    stage_id,
    session_id,
);
spawn_in_terminal(&self.terminal, &title, ..., &attach_cmd, ..., Some(stage_id))?;
```

Spawn order matters:

1. `docker run -d ...` — container starts, returns immediately.
2. Wait for the container to actually be running (`docker inspect` until `State.Running == true`, brief poll loop with a timeout). This handles the small race where `exec` runs before the container is fully up.
3. `spawn_in_terminal` opens the host window and runs `docker exec -it`.

Closing the terminal window does **not** kill the container; the container outlives the attached terminal. `kill_session` always runs `rm -f` against the container regardless of terminal state.

For headless (auto-mode swarm) operation, add `loom run --no-attach` which skips the terminal spawn entirely; `loom status --live` is enough to monitor.

### 6.8 PID tracking

The container has its own PID namespace. On Docker and Podman, the container's main process is visible to the host via `docker inspect -f '{{.State.Pid}}' loom-{stage_id}`; write that host-side PID to `.work/pids/{stage_id}.pid` so `is_process_alive` (which uses `kill -0`) keeps working unmodified.

On Apple Container the container runs in its own VM and the host kernel does not see the container's main process as a sibling PID. `kill -0` against the inspect output will fail. For Apple Container, skip the PID file entirely and rely on `is_session_alive`'s `inspect -f '{{.State.Running}}'` path, which works on every runtime. The orchestrator's existing PID-based liveness check should fall through to the inspect path when the PID file is absent or stale; verify this in `is_session_alive` for the container backend.

### 6.9 Per-stage backend dispatch

Sections 11.5 and 11.6 commit to mixed mode (some stages native, some container in the same run). The orchestrator currently calls `create_backend()` once at startup; that pattern doesn't support per-stage backend selection. Phase 2 introduces a dispatcher.

`loom/src/orchestrator/terminal/dispatcher.rs` (new):

```rust
use std::path::Path;
use anyhow::Result;
use crate::plan::schema::execution::BackendType;
use super::{create_backend, TerminalBackend};

/// Holds an instance of every backend the project might use, and routes
/// each call to the correct one based on the stage's resolved backend.
///
/// Constructed once per `loom run`; lives for the duration of the run.
pub struct BackendDispatcher {
    native: Option<Box<dyn TerminalBackend>>,
    container: Option<Box<dyn TerminalBackend>>,
}

impl BackendDispatcher {
    /// Build the set of backends the plan actually needs.
    /// Inspecting all stages up-front avoids constructing backends that
    /// are never used (e.g. if no stage opts into native within a
    /// container project, we don't construct NativeBackend).
    pub fn for_plan(
        project_backend: BackendType,
        needs: BackendNeeds,
        work_dir: &Path,
    ) -> Result<Self> {
        Ok(Self {
            native: if needs.native { Some(create_backend(BackendType::Native, work_dir)?) } else { None },
            container: if needs.container { Some(create_backend(BackendType::Container, work_dir)?) } else { None },
        })
    }

    pub fn for_stage(&self, backend: BackendType) -> &dyn TerminalBackend {
        match backend {
            BackendType::Native => self.native.as_deref().expect("native backend not initialised"),
            BackendType::Container => self.container.as_deref().expect("container backend not initialised"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BackendNeeds {
    pub native: bool,
    pub container: bool,
}
```

Resolution helper for a single stage's backend:

```rust
pub fn resolve_stage_backend(
    project_backend: BackendType,
    stage_override: Option<BackendType>,
) -> Result<BackendType> {
    match (project_backend, stage_override) {
        // No override: use project default.
        (project, None) => Ok(project),
        // Stage matches project: trivially fine.
        (project, Some(stage)) if stage == project => Ok(project),
        // Container project, native stage: legal narrowing.
        (BackendType::Container, Some(BackendType::Native)) => Ok(BackendType::Native),
        // Native project, container stage: illegal widening.
        (BackendType::Native, Some(BackendType::Container)) => bail!(
            "stage requires execution.backend=container, but project backend is native. \
             Re-initialise with: loom init <plan> --backend container"
        ),
    }
}
```

`loom init` walks every stage at validation time, calls `resolve_stage_backend`, and rejects the plan if any stage demands container in a native project. This catches the error at init time rather than mid-run.

At run time, the orchestrator:

1. Reads `project_backend` from `.work/config.toml`.
2. Walks the plan, resolves each stage's backend via `resolve_stage_backend`, and computes `BackendNeeds` (which backends actually get used).
3. Constructs a `BackendDispatcher` with only the needed backends.
4. For each session spawn, calls `dispatcher.for_stage(stage_backend).spawn_session(...)`.

The orchestrator code that currently holds a `Box<dyn TerminalBackend>` is updated to hold a `BackendDispatcher` and to look up the per-stage backend before each `spawn_*` call. Callsites are in `orchestrator/core/` (the dispatch loop) and `orchestrator/monitor/` (re-spawn after recovery).

### 6.10 Acceptance criteria for Phase 2

- `loom init <plan> --backend container` followed by `loom run` launches one container per stage, bind-mounts the worktree, and runs Claude inside it. (For Phase 2 development the image may be hand-built and the digest hand-edited into `.work/config.toml`; Phase 3 automates this.)
- `loom status` shows the same state for container-backed stages as for native-backed.
- `loom stop` terminates all containers cleanly.
- All existing hooks (file isolation, commit guard) still trigger inside the container.
- Container has no host filesystem access outside the worktree, `.work/`, and `~/.claude/.credentials.json`.
- A plan with one container stage and one native stage in a container-mode project runs both correctly via the dispatcher.
- A plan with a container-only stage in a native-mode project is rejected at `loom init` with the actionable error.

---

## 7. Phase 3: Language fingerprinting and global image cache

### 7.1 Design

There is no `loom container init` subcommand. Container setup is folded into `loom init`.

Images are not per-project. Two Rust projects produce identical Dockerfiles and would build identical images; this is wasteful. Instead, images are cached globally on the host, keyed by a **language fingerprint** computed from the project's detected languages. The first `loom init --backend container` for a given fingerprint pays the build cost; every subsequent project with the same fingerprint reuses the cached image.

The per-project `.work/config.toml` stores only a digest reference into that global cache.

### 7.2 Cache layout

Use the platform cache directory via the `dirs` crate (already a loom dependency):

```
$LOOM_CACHE_DIR/loom/images/<fingerprint>/
├── Dockerfile         (generated)
├── firewall.sh        (generated, copied into image)
├── image.digest       (sha256:...)
└── built_at           (RFC3339 timestamp)
```

Where `$LOOM_CACHE_DIR` resolves to:

- Linux: `$XDG_CACHE_HOME/loom/images/` (default `~/.cache/loom/images/`)
- macOS: `~/Library/Caches/loom/images/`

Image tag convention: `loom/base:<fingerprint>` plus a digest-based pin.

### 7.3 Language fingerprint

`loom/src/orchestrator/terminal/container/fingerprint.rs`:

```rust
use crate::language::{detect_project_languages, DetectedLanguage};
use sha2::{Digest, Sha256};
use std::path::Path;

const DOCKERFILE_TEMPLATE: &str = include_str!("../../../../resources/Dockerfile.tmpl");

/// Stable fingerprint string derived from detected project languages
/// AND the Dockerfile template content. The template hash invalidates
/// the cache when loom ships a new template (e.g. new system tools).
///
/// Format: "<langs>-<dockerfile-hash[:8]>"
/// Examples: "rust-a1b2c3d4", "rust-typescript-a1b2c3d4"
pub fn compute_fingerprint(project_root: &Path) -> String {
    let mut langs = detect_project_languages(project_root);
    langs.sort_by_key(|l| l.canonical_name());
    langs.dedup_by_key(|l| l.canonical_name());

    let lang_part = if langs.is_empty() {
        "base".to_string()
    } else {
        langs.iter()
            .map(|l| l.canonical_name())
            .collect::<Vec<_>>()
            .join("-")
    };

    let mut hasher = Sha256::new();
    hasher.update(DOCKERFILE_TEMPLATE.as_bytes());
    let template_hash = hex::encode(&hasher.finalize()[..4]);  // 8 hex chars

    format!("{lang_part}-{template_hash}")
}
```

`DetectedLanguage::canonical_name` returns lowercase short names: `"rust"`, `"typescript"`, `"python"`, `"go"`, etc. Add this method if it doesn't exist.

The fingerprint is intentionally coarse on the language axis (per-project package installation happens at runtime inside the container, on the bind-mounted `/workspace`), but precise on the template axis: any change to the Dockerfile template — a new system tool, a base-image bump, a security patch in the template — produces a fresh fingerprint and triggers a clean rebuild on next `loom init`. This avoids the trap of users on an old loom version silently using a stale image.

Add `sha2` and `hex` to `loom/Cargo.toml` if not already present.

### 7.4 `loom init` flag semantics

Extend `loom init` with two flags:

```
loom init <plan> [--backend native|container] [--no-build]
```

Behaviour matrix:

| Situation                                       | Action                                                                                          |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Fresh project, no `--backend`                   | Backend = native. As today.                                                                     |
| Fresh project, `--backend container`            | Detect runtime → fingerprint → check cache → build if absent → pin.                             |
| Fresh project, `--backend container --no-build` | Detect runtime → fingerprint → check cache → if absent, write a "pending" marker; do not build. |
| Existing project, no `--backend`                | Preserve existing backend in `.work/config.toml`. Re-parse plan as today.                       |
| Existing project, `--backend container`         | Switch to container mode. Re-detect fingerprint; build/pin as needed.                           |
| Existing project, `--backend native`            | Switch to native mode. Drop digest pin from config.                                             |
| `--backend container` and no runtime installed  | Hard fail with installation instructions for the host OS. No partial state written.             |

The hard fail is deliberate. If the user asked for container mode, they want to know about a missing runtime now, not three days later when the first stage tries to spawn.

### 7.5 What `loom init --backend container` does, step by step

1. **Validate the plan.** Same parsing/validation loom already does; no changes.
2. **Detect runtime.** Call `runtime::detect_runtime("auto")`. On failure, emit OS-specific install instructions and exit non-zero. Sample output for Linux:

   ```
   error: container backend requested but no runtime found

   Install one of:
     Podman (recommended):  sudo apt install podman      # Debian/Ubuntu
                            sudo dnf install podman      # Fedora
     Docker:                https://docs.docker.com/engine/install/

   Then re-run: loom init <plan> --backend container
   ```

3. **Compute fingerprint.** From detected languages.
4. **Check global cache.** Look up `$LOOM_CACHE_DIR/loom/images/<fingerprint>/image.digest`.
   - **Hit:** Verify the image is still present in the local runtime (`docker image inspect <digest>`). If yes, copy the digest into `.work/config.toml`. Done.
   - **Miss:** Render Dockerfile and firewall script into the cache directory. Build the image, capturing progress on stdout. On success, write `image.digest` and `built_at`. Then copy the digest into `.work/config.toml`.
5. **Write `.work/config.toml`:**

   ```toml
   [execution]
   backend = "container"

   [execution.container]
   runtime = "podman"           # resolved at init time
   fingerprint = "rust-typescript"
   image_digest = "sha256:..."
   forward_credentials = []
   ```

6. **Print summary.** Backend, runtime, fingerprint, whether the image was cached or freshly built, and the elapsed time for the build (if any).

### 7.6 Build progress

The build is blocking. Stream the runtime's native build output to stdout (`docker build` and `podman build` both emit progress; use `--progress=plain` for legibility). Do not wrap in a fancy spinner; the runtime's output is more useful for debugging build failures.

Typical first-time output:

```
$ loom init plan.md --backend container
✓ plan validated: 12 stages
✓ runtime detected: podman 5.2.0
✓ language fingerprint: rust-typescript
  no cached image for fingerprint, building...

[+] Building loom/base:rust-typescript
 => [internal] load build definition                              0.1s
 => [1/8] FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04  3.2s
 => [2/8] RUN apt-get update && apt-get install -y ...           42.1s
 => [3/8] RUN curl ... rustup ...                                89.4s
 => [4/8] RUN npm install -g @anthropic-ai/claude-code           18.7s
 ... (continues) ...
 => exporting to image                                            5.3s

✓ image built: sha256:abc123def456... (3m 24s)
✓ pinned digest in .work/config.toml

container backend ready. Run `loom run` to start.
```

### 7.7 `--no-build` behaviour

When `--no-build` is passed and the image isn't cached, write `.work/config.toml` with `image_digest = "pending"` and emit:

```
⚠ image not built. Run `loom container build` before `loom run`.
```

`loom run` checks for `image_digest = "pending"` and refuses with an actionable error. There is no auto-build at run time.

`--no-build` exists for users who want to script the init separately (e.g. `loom init` in CI bootstrap, then a parallel image build, then `loom run` once both finish). It is not the recommended path.

### 7.8 The `loom container` subcommand

A small set of utility commands for managing the global cache. None of them are required for normal use; `loom init` covers the common case.

```
loom container build [--fingerprint <fp>]
    Build the image for the given fingerprint, or for the current
    project's fingerprint if omitted. Useful with --no-build init,
    or after editing the Dockerfile template (Phase 3 ships a fixed
    template, but future versions may allow user customisation).

loom container rebuild [--fingerprint <fp>] [--all]
    Force a rebuild even if cached. Use after upgrading loom or
    after a base-image security update.

loom container doctor
    Diagnostic dump: runtime, runtime version, current project's
    fingerprint and pinned digest, image presence in local runtime,
    cache directory size, list of all cached fingerprints.

loom container shell <stage-id>
    Open an interactive shell inside a running stage's container.
    For incident response and debugging.
```

`loom container clean` (purge cached images) is intentionally omitted to avoid accidental footguns. Users can `docker rmi` directly if needed.

### 7.9 Re-detection on subsequent `loom init`

Every `loom init` re-runs language detection. If the resulting fingerprint differs from the one pinned in `.work/config.toml`, loom prints a clear notice and updates the pin:

```
⚠ project languages changed: rust → rust-typescript
  using cached image for new fingerprint (built 2 days ago)
  updated .work/config.toml
```

If the new fingerprint isn't cached and `--no-build` was not passed, the build proceeds automatically. If `--no-build` was passed, the digest goes to `pending` and `loom run` will refuse.

This means there is no separate `--refresh` flag; re-running `loom init` is the refresh.

### 7.10 Dockerfile template

Same as the previous draft (see Section 7.2 of the prior version, retained below for reference). The template is shipped inside the loom binary via `include_str!`, rendered with the detected languages, and written to `$LOOM_CACHE_DIR/loom/images/<fingerprint>/Dockerfile` at build time.

```dockerfile
FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

ARG USERNAME=loom
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME -s /bin/bash \
    && apt-get update \
    && apt-get install -y sudo iptables ipset dnsutils ripgrep fd-find git curl ca-certificates e2fsprogs \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @anthropic-ai/claude-code

# Per-language toolchains (rendered conditionally from fingerprint)
{{#if has_rust}}
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
{{/if}}

{{#if has_typescript}}
RUN npm install -g bun pnpm
{{/if}}

{{#if has_python}}
RUN apt-get update && apt-get install -y python3 python3-pip python3-venv && rm -rf /var/lib/apt/lists/*
{{/if}}

{{#if has_go}}
RUN curl -fsSL https://go.dev/dl/go1.23.linux-amd64.tar.gz | tar -C /usr/local -xz
ENV PATH="/usr/local/go/bin:${PATH}"
{{/if}}

# Firewall script: rendered per project at run time (Phase 4), bind-mounted in.
# This Dockerfile installs the helper that reads the runtime-mounted policy.
COPY firewall.sh /usr/local/bin/loom-firewall.sh
RUN chmod +x /usr/local/bin/loom-firewall.sh && chattr +i /usr/local/bin/loom-firewall.sh || true

RUN echo "$USERNAME ALL=(ALL) NOPASSWD: /usr/local/bin/loom-firewall.sh" >> /etc/sudoers
USER $USERNAME
WORKDIR /workspace

ENTRYPOINT ["/bin/bash", "-lc", "sudo /usr/local/bin/loom-firewall.sh && exec \"$@\"", "--"]
CMD ["/bin/bash"]
```

Note one architectural shift versus Phase 4 in the previous draft: the firewall script baked into the image is a generic bootstrap that reads the per-project allowlist from a bind-mounted file at run time. This means the global image cache stays project-agnostic. Phase 4 details how the per-project allowlist is delivered.

### 7.11 Acceptance criteria for Phase 3

- `loom init <plan> --backend container` on a fresh machine fails fast with install instructions when no runtime is present.
- With a runtime present, the same command builds the image (showing build progress), pins the digest, writes `.work/config.toml`, and exits successfully.
- Running `loom init <plan2> --backend container` on a second project with the same language fingerprint completes in milliseconds and reuses the cached image.
- Running `loom init <plan>` on the same project a second time without `--backend` preserves the container backend.
- Running `loom init <plan> --backend native` on a previously-container project drops the digest from `.work/config.toml` and switches to native.
- Adding a language to a project (e.g. introducing a `package.json` to a Rust project) and re-running `loom init` triggers a fingerprint change notice, and either reuses a cached image for the new fingerprint or builds it.
- `loom container doctor` lists the cache directory contents and reports the current project's fingerprint and digest.
- `--no-build` writes `image_digest = "pending"`. `loom run` then refuses with an actionable error.

---

## 8. Phase 4: Network egress firewall

### 8.1 Strategy

The firewall script in the image is generic. It reads the per-project allowlist from a bind-mounted policy file at container startup, then sets up iptables/ipset rules. This keeps the global image cache project-agnostic; per-project network policy lives in `.work/`.

At each container start:

1. The image-resident `/usr/local/bin/loom-firewall.sh` runs (via the entrypoint).
2. It reads the allowlist from `/work/network/allowed_domains.txt` (bind-mounted from `.work/network/allowed_domains.txt`).
3. Drops all outbound by default.
4. Allows DNS to the container runtime's embedded resolver.
5. Resolves each allowlisted domain, populates an ipset.
6. Allows outbound to the ipset; rejects everything else.

The allowlist file is generated by `loom/src/orchestrator/terminal/container/network.rs` from `MergedSandboxConfig.network` whenever `loom run` (or any session-spawning command) starts. It's a flat newline-separated list of domain names, easy to inspect and diff.

### 8.2 The firewall script (image-resident, generic)

`$LOOM_CACHE_DIR/loom/images/<fingerprint>/firewall.sh`, copied into the image at build time:

```bash
#!/bin/bash
set -euo pipefail

POLICY_FILE="/work/network/allowed_domains.txt"

# Always-required domains (Anthropic API, npm registry for Claude Code self-update)
ALWAYS=("api.anthropic.com" "registry.npmjs.org")

DOMAINS=("${ALWAYS[@]}")
if [[ -r "$POLICY_FILE" ]]; then
  while IFS= read -r line; do
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    DOMAINS+=("$line")
  done < "$POLICY_FILE"
fi

ipset create loom-allowed hash:ip family inet -exist
ipset flush loom-allowed

for domain in "${DOMAINS[@]}"; do
  for ip in $(getent ahosts "$domain" | awk '{print $1}' | sort -u); do
    ipset add loom-allowed "$ip" -exist
  done
done

iptables -F OUTPUT
iptables -P OUTPUT DROP
iptables -A OUTPUT -o lo -j ACCEPT
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 53 -j ACCEPT
iptables -A OUTPUT -m set --match-set loom-allowed dst -j ACCEPT
iptables -A OUTPUT -j REJECT --reject-with icmp-net-unreachable

echo "loom firewall ready: $(ipset list loom-allowed | grep -c '^[0-9]') IPs allowed" 1>&2
```

The script is `chattr +i`-ed in the image, and the bind-mounted policy file is mounted read-only. An in-container Claude session cannot disable the firewall or rewrite the allowlist even with `sudo` (the sudoers rule only permits running the firewall script; the policy file itself is `:ro`).

### 8.3 Per-project allowlist generation

In `network.rs`:

```rust
pub fn write_allowlist(work_dir: &Path, network: &NetworkConfig) -> Result<PathBuf> {
    let dir = work_dir.join("network");
    fs::create_dir_all(&dir)?;
    let path = dir.join("allowed_domains.txt");

    let mut content = String::new();
    content.push_str("# Generated by loom; do not edit by hand.\n");
    content.push_str("# Configure via plan: loom.sandbox.network.allowed_domains\n");
    for domain in &network.allowed_domains {
        content.push_str(domain);
        content.push('\n');
    }
    for domain in &network.additional_domains {
        content.push_str(domain);
        content.push('\n');
    }
    fs::write(&path, content)?;
    Ok(path)
}
```

Called from `ContainerBackend::spawn_session` (and the merge/knowledge variants) before launching the container. The file is regenerated on every session spawn, picking up any plan edits.

### 8.4 Apple Container caveat

Apple Container runs each container in its own VM, so `--cap-add=NET_ADMIN` is a no-op and iptables inside the container only affects that VM's network. The script still works; it's just doing layered defence on top of the VM boundary. No code changes needed for macOS.

### 8.5 Per-stage networks

Before each `docker run`, ensure the per-stage bridge exists:

```rust
fn ensure_network(runtime: Runtime, stage_id: &str) -> Result<()> {
    let name = format!("loom-net-{stage_id}");
    let exists = Command::new(runtime.binary())
        .args(["network", "inspect", &name])
        .output()?
        .status
        .success();
    if !exists {
        Command::new(runtime.binary())
            .args(["network", "create", "--driver=bridge", &name])
            .status()
            .context("Failed to create per-stage network")?;
    }
    Ok(())
}
```

`kill_session` removes the network after `rm -f` of the container.

### 8.6 iptables compatibility

Ubuntu 24.04's `iptables` command is iptables-nft by default; the script uses legacy iptables syntax which works through the nft compatibility shim. The image must have `iptables` and `ipset` installed (the Phase 3 Dockerfile installs them via `apt-get install`).

### 8.7 Network policy file integrity

The policy file at `.work/network/allowed_domains.txt` is bind-mounted as part of `/work` (read-write, since `.work/` as a whole needs writes for sessions, signals, etc.). The firewall already loads its rules into the kernel at container startup, before Claude is spawned; modifying the policy file post-startup does not reload iptables. So the firewall is defended by the order of operations, not by file permissions. If a stronger guarantee is desired in a future revision, mount just the policy file separately as `:ro`.

### 8.8 Acceptance criteria for Phase 4

- A container with `network.allowed_domains: ["github.com"]` can `curl https://github.com` but cannot `curl https://example.com`.
- A container with empty `allowed_domains` can still reach `api.anthropic.com` and `registry.npmjs.org` (the always-required set).
- Multiple stages running concurrently are on separate bridge networks and cannot reach each other.
- The firewall script is immutable inside the container (`chattr +i`); the limited sudoers entry permits only execution of the script, not editing it. Attempting `sudo vim /usr/local/bin/loom-firewall.sh` is denied at the sudoers layer; even with full root, `chattr +i` prevents writes.

---

## 9. Phase 5: macOS path (Apple Container)

### 9.1 Detection priority

On macOS, prefer Apple Container (`/usr/local/bin/container`) over Docker Desktop. Apple Container ships with recent Xcode, gives each container its own VM by default, and avoids the Docker Desktop licence question.

`runtime::detect_auto` already orders the macOS branch correctly. Verify:

```rust
#[cfg(target_os = "macos")]
fn detect_auto() -> Result<Runtime> {
    if find_binary("container").is_ok() { return Ok(Runtime::AppleContainer); }
    if find_binary("podman").is_ok() { return Ok(Runtime::Podman); }
    if find_binary("docker").is_ok() { return Ok(Runtime::Docker); }
    bail!("...")
}
```

### 9.2 Apple Container CLI quirks

Apple Container's CLI is mostly Docker-compatible but has a few differences relevant here:

- `--cap-add` is silently ignored (each container is its own VM, so capabilities are moot).
- `--network` accepts the same syntax but per-stage networks may be implemented as VLAN tags rather than bridges.
- `inspect -f` works the same way.
- Bind mounts work but the syntax is `--mount type=bind,source=...,target=...` rather than `-v` (older Docker syntax). Use the `--mount` form throughout for portability.

Use `--mount` form everywhere to avoid runtime-specific code paths:

```rust
format!("--mount=type=bind,source={},target=/workspace", worktree_abs.display())
```

### 9.3 Image build on macOS

Apple Container reads OCI images. The same Dockerfile builds and runs unchanged. `container build -t loom/{slug}:latest .` works in place of `docker build`.

### 9.4 Acceptance criteria for Phase 5

- `loom init <plan> --backend container` on macOS detects Apple Container, builds the image with `container build`, pins the digest in `.work/config.toml`.
- `loom run` on macOS launches stages in Apple Container VMs.
- macOS CI in the loom repo runs at least a smoke test of `loom init --backend container` (build-only, no full session run).

---

## 10. Phase 6: bypassPermissions opt-in

### 10.1 Unlock condition

`validate_config` already rejects `bypassPermissions` on native. Phase 6 documents and tests the container-allowed path. No code change beyond:

1. Plan author writes `sandbox.permission_mode: bypassPermissions` and `execution.backend: container`.
2. `validate_config` returns Ok.
3. `apply_default_mode` writes `"defaultMode": "bypassPermissions"` to `.claude/settings.local.json`.
4. Container starts. Claude runs with no permission prompts.

### 10.2 When to use bypass

| Stage profile                            | Recommended mode    |
| ---------------------------------------- | ------------------- |
| Unattended overnight implementation runs | `bypassPermissions` |
| CI-driven verification                   | `bypassPermissions` |
| Daytime interactive supervision          | `auto`              |
| Knowledge gathering or distillation      | `acceptEdits`       |

`auto` should be the documented default. `bypassPermissions` is for users who have read the threat model and accepted that prompt-injection-driven exfiltration is now bounded by the firewall, not by per-tool prompts.

### 10.3 Acceptance criteria for Phase 6

- A documented end-to-end example runs an entire plan unattended with `bypassPermissions` and produces a clean merge.
- Switching the same plan back to `auto` works without code changes.
- Switching to `bypassPermissions` with `execution.backend: native` fails with a clear error.

---

## 11. User experience

### 11.1 The happy path

A user with a runtime already installed (Podman/Docker on Linux, Apple Container/Docker on macOS) needs only:

```
loom init my-plan.md --backend container
loom run
```

`loom init` validates the plan, detects languages, computes the fingerprint, builds the image (first time for that language combo on this machine; subsequently cached), pins the digest, and writes `.work/config.toml`. Total time on first run: typically 3-10 minutes for the build, dominated by package installs (rustup is ~600 MB on its own; apt + npm + bun add several more). Slow on constrained networks. On second and subsequent projects with the same languages, fingerprint hits the cache: under a second.

Once the image is built, Claude Code inside it auto-updates itself by default (the Anthropic devcontainer feature behaviour), so the cached image's Claude version is a _floor_, not a fixed pin. Don't try to pin it; the in-container update is the supported path.

`loom run` works as it does today; the orchestrator notices the container backend in `.work/config.toml` and routes session spawns through `ContainerBackend`. No additional flags.

### 11.2 No runtime installed

```
$ loom init my-plan.md --backend container
error: container backend requested but no runtime found

Install one of:
  Podman (recommended):  sudo apt install podman      # Debian/Ubuntu
                         sudo dnf install podman      # Fedora
  Docker:                https://docs.docker.com/engine/install/

Then re-run: loom init my-plan.md --backend container
```

No partial state is written; the user installs a runtime and re-runs the same command.

### 11.3 Switching an existing project's backend

```
loom init my-plan.md --backend native     # switch back to native
loom init my-plan.md --backend container  # switch to container; re-pins digest
loom init my-plan.md                      # preserves whichever was set last
```

### 11.4 Plan-level configuration (optional)

The flag handles backend selection. Plan-level config covers the rest: permission mode, network allowlist, filesystem rules, credential forwarding.

```yaml
loom:
  version: 1
  execution:
    container:
      forward_credentials: ["github"] # passes GH_TOKEN through (only effective when project is in container mode)
  sandbox:
    permission_mode: auto # default for Standard stages anyway
    network:
      allowed_domains: ["github.com", "crates.io", "static.crates.io"]
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**"]
```

The `execution.container` block is honoured only when the project has been initialised with `--backend container`. On a native project the block is silently ignored, never auto-promoted. This is deliberate: cloning someone else's plan must never silently change your execution model.

### 11.5 Stage-level override example (within a container-mode project)

```yaml
stages:
  - id: implement-feature-x
    type: standard
    sandbox:
      permission_mode: bypassPermissions # only valid because backend resolves to container
    execution:
      backend: container # explicit; matches project default

  - id: simulator-integration
    type: standard
    execution:
      backend: native # opts this stage out into native; runs against host simulator
```

A stage requesting `backend: container` in a project initialised with `--backend native` is rejected at `loom init` validation time. There is no implicit promotion.

### 11.6 Mixed mode (some stages native, some container)

Supported. Each stage's resolved backend is independent. Useful for stages that need host tooling that's awkward to containerise (e.g., a stage that runs against a host-installed simulator).

---

## 12. Migration

### 12.1 Existing users

- Phase 1 ships. Existing plans automatically get auto mode for Standard stages. No action required. If a user prefers acceptEdits behaviour, they add `permission_mode: acceptEdits` at the plan level.
- Phases 2-6 ship. Default backend remains `native`. No existing plan changes behaviour. Users opt into containers explicitly.

### 12.2 Telemetry / diagnostics

Add to `loom diagnose`:

- Resolved permission mode per stage.
- Resolved backend per stage.
- Container runtime (if applicable), image digest, firewall status.
- Sample of in-container env vars (sanitised).

### 12.3 Rollback

If a Phase 1 release causes problems, users can pin auto-mode off globally:

```yaml
loom:
  sandbox:
    permission_mode: acceptEdits # plan-level forces all stages
```

---

## 13. Testing strategy

### 13.1 Unit tests

- `merge_config` for every `(plan_mode, stage_mode, stage_type)` combination including `None`.
- `validate_config` for all `(mode, backend)` pairs.
- `runtime::detect_auto` with mocked `which` results for each OS.
- `build_run_args` snapshot tests against a known set of inputs.
- `firewall.sh` template generation snapshot tests.

### 13.2 Integration tests (gated on container runtime presence)

`loom/tests/container/` (new):

- `test_spawn_stage_in_container`: run a stage that prints `pwd` and asserts `/workspace`.
- `test_no_host_filesystem_access`: stage tries to read `/etc/shadow`, asserts EACCES (or absent because of bind-mount scope).
- `test_egress_blocked`: stage curls a non-allowlisted domain, asserts failure.
- `test_egress_allowed`: stage curls `api.anthropic.com`, asserts success.
- `test_kill_session_removes_container`: spawn, kill, assert `<runtime> ps` shows no `loom-*` containers (where `<runtime>` is the configured runtime — docker, podman, or container).
- `test_concurrent_stages_isolated`: two stages, one writes to `/tmp/x`, the other can't see it.
- `test_mixed_mode_dispatch`: a plan with one container stage and one native stage in a container-mode project; both run, dispatcher routes correctly.
- `test_native_project_rejects_container_stage`: plan with a stage demanding container in a native project; `loom init` exits non-zero with the expected error message.
- `test_uid_mismatch_credentials_readable`: spawn a container as a host user with UID != 1000, verify the bind-mounted `~/.claude/.credentials.json` is readable inside.

These run only when `LOOM_TEST_CONTAINERS=1` is set in CI to avoid coupling unit tests to a runtime. Within those tests, the runtime is selected via `runtime::detect_runtime("auto")` and used uniformly.

### 13.3 Manual acceptance run

A documented acceptance procedure for each release:

1. Fresh clone of a sample project on a machine with no container runtime.
2. `loom init plan.md --backend container` → expect hard-fail with install instructions.
3. Install Podman (Linux) or Apple Container (macOS).
4. `loom init plan.md --backend container` → expect successful build, digest pinned.
5. `loom run` → expect all stages to complete unattended.
6. Verify `docker ps` (or runtime equivalent) and `docker network ls` are clean afterwards.
7. Verify `~/.ssh` is unreadable from inside any stage container (run `loom container shell <stage-id>` mid-run, or repeat run with a `cat ~/.ssh/id_ed25519` smoke stage).
8. On a second sample project with the same languages, run `loom init plan.md --backend container` and verify it completes in under a second (cache hit).

---

## 14. Open questions

1. **Image distribution.** Should Loom publish base images to a public registry (`ghcr.io/cosmix/loom-base:rust-typescript`) so users can `docker pull` instead of building? Pros: skips the 3-5 minute first-run build entirely. Cons: registry is another release surface; image needs to be trustworthy. Defer until after Phase 3 ships and we see how often users hit the build cost.
2. **Container reuse across stages.** Currently one container per stage, started fresh. An alternative: one container per worktree, with `docker exec` reused across stages. Faster but harder to firewall consistently (firewall is initialised once per container). Defer.
3. **Resource limits.** Should `ContainerConfig` expose `--memory`, `--cpus`? Probably yes in a follow-up; not blocking. Useful when running 10+ stages in parallel on a constrained host.
4. **macOS Docker Desktop fallback path.** If a user has only Docker Desktop installed (no Apple Container), `loom init --backend container` succeeds via `runtime: docker`. Document the security tradeoff: shared kernel via Docker Desktop's VM, but the VM is the boundary, so it's acceptable. Apple Container gives a per-container VM, which is strictly better.
5. **Subagents and team mode.** `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is forwarded. Subagents inherit the container, which is correct. Verify in integration tests.
6. **Cache eviction.** The global image cache grows over time as users work on projects with new language combinations. Should `loom container doctor` warn when total cache exceeds (e.g.) 10 GB? Should there be a `loom container prune --older-than 30d`? Probably yes in a follow-up; defer.

---

## 15. Out-of-scope items, captured for later

- Firecracker microVMs as a third backend.
- Remote container hosts (Docker contexts).
- Kubernetes-based execution.
- Persistent volumes for build caches across stage runs (could use a named volume at `/home/loom/.cache`).
- Image signing and verification (cosign).
- A managed-policy file equivalent to Claude Code's `managed-settings.json`.

---

## 16. File-by-file change summary

| Area                                      | Files                                                                                                                           | Phase |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- | ----- |
| `PermissionMode` enum                     | `loom/src/plan/schema/types.rs`                                                                                                 | 1     |
| `BackendType` enum + impls                | `loom/src/plan/schema/execution.rs` (new; `Container` variant present from day 1, only `Native` is constructible until Phase 2) | 1     |
| Plan-level + stage-level execution config | `loom/src/plan/schema/execution.rs`, `loom/src/plan/schema/mod.rs`, stage schema                                                | 1     |
| `MergedSandboxConfig` extension           | `loom/src/sandbox/config.rs`                                                                                                    | 1     |
| Settings writers                          | `loom/src/sandbox/settings.rs`, `loom/src/git/worktree/settings.rs`, `loom/src/hooks/generator.rs`                              | 1     |
| `BackendType` re-export                   | `loom/src/orchestrator/terminal/mod.rs` (replaces the previous local enum)                                                      | 2     |
| `create_backend` `Container` arm          | `loom/src/orchestrator/terminal/mod.rs`                                                                                         | 2     |
| Container backend                         | `loom/src/orchestrator/terminal/container/{mod,runtime,lifecycle}.rs`                                                           | 2     |
| Per-stage backend dispatcher              | `loom/src/orchestrator/terminal/dispatcher.rs` (new)                                                                            | 2     |
| Orchestrator dispatch update              | `loom/src/orchestrator/core/*` (hold `BackendDispatcher`, look up per stage on each spawn)                                      | 2     |
| Container-relative wrapper script         | `loom/src/orchestrator/terminal/native/pid_tracking.rs` (additive)                                                              | 2     |
| `loom init --backend` flag                | `loom/src/cli/{dispatch,types}.rs`, `loom/src/commands/init/*`                                                                  | 3     |
| Language fingerprint                      | `loom/src/orchestrator/terminal/container/fingerprint.rs`, `loom/src/language.rs` (extend)                                      | 3     |
| Global image cache                        | `loom/src/orchestrator/terminal/container/image.rs`, `loom/resources/Dockerfile.tmpl`                                           | 3     |
| `loom container` utility commands         | `loom/src/commands/container/{build,rebuild,doctor,shell}.rs`                                                                   | 3     |
| `.work/config.toml` schema                | `loom/src/fs/work_dir.rs` (extend the existing config struct, use `toml_edit` for round-tripping)                               | 3     |
| Firewall                                  | `loom/src/orchestrator/terminal/container/network.rs`, `loom/resources/firewall.sh`                                             | 4     |
| macOS / Apple Container                   | `loom/src/orchestrator/terminal/container/runtime.rs` (cfg branches)                                                            | 5     |
| Bypass-mode docs and tests                | `README.md`, `loom/src/sandbox/config.rs` (tests)                                                                               | 6     |

---

## 17. Implementation tips for the agent doing this work

- Implement Phase 1 in a single PR; keep it small and easy to review. The schema change, the resolver, the writer change, and tests fit comfortably in ~250 lines. Phase 1 ships `BackendType` even though only `Native` is constructible at run time; `Container` is reserved for Phase 2.
- For Phase 2, get `spawn_session` working end-to-end with a hand-built image first. Don't block on Phase 3 (image build automation). The integration point is `BackendType::Container` selection in `.work/config.toml`; you can hand-edit that file during Phase 2 development.
- Reuse the existing `pid_tracking::create_wrapper_script` rather than duplicating it. Add a `paths: WrapperPaths` parameter that holds either host paths (current behaviour) or container paths (new).
- The `kill_session` window-title fallback in `NativeBackend` is irrelevant for `ContainerBackend`; don't port it. `<runtime> rm -f` is sufficient.
- Treat the firewall as defence in depth, not the primary boundary. The primary boundaries are: (a) container kernel/namespace isolation, (b) bind mount scope, (c) explicit env-var stripping. The firewall stops prompt-injection-driven exfiltration. Document this in `README.md` so users don't over-rely on it.
- The `chattr +i` on the firewall script needs `e2fsprogs` in the image; the Dockerfile in Section 7.10 installs it. On Apple Container the in-VM filesystem is ext4 by default; verify `chattr +i` succeeds during image build (the Dockerfile uses `|| true` to tolerate filesystems that don't support it).
- `loom init` validation must happen before any image build. Failing fast on a bad plan saves the user 5-10 minutes of needless build time.
- The fingerprint computation should be deterministic across runs; sort and dedup before joining, and include the Dockerfile template content hash. A non-deterministic fingerprint defeats the cache; a fingerprint that ignores the template silently runs stale images after loom upgrades.
- Use the `toml_edit` crate (not the simpler `toml` crate) for `.work/config.toml` writes. `toml_edit` preserves comments, formatting, and unknown fields. The user might have hand-added something; `loom init` should not eat it.
- Add a `loom container shell <stage-id>` debug command that opens a shell in a running stage's container, for incident response. Trivial to add and saves time when something goes wrong.
- The `container` subcommand utilities (`build`, `rebuild`, `doctor`, `shell`) are auxiliary; the main user flow is `loom init --backend container` + `loom run`. Don't overweight the subcommand docs in the README.
- UID mapping (Section 6.5) is the most likely source of subtle bugs. Test on a host with UID != 1000 (e.g., the default UID on some CI images is 1001 or higher). Without correct UID handling, `~/.claude/.credentials.json` reads will silently fail and the in-container Claude will hit auth errors.
- `is_session_alive` for `ContainerBackend` should not depend on `kill -0` against the host PID because that fails on Apple Container. Drive liveness off `<runtime> inspect -f '{{.State.Running}}'`. Keep the PID file for Docker/Podman as supplementary diagnostic info, not as the primary signal.
