# Code Review: Container Backend for Loom Sessions

**Plan:** PLAN-container-backend | **Generated:** 2026-05-11 13:49 UTC

## Summary

Adds a production-grade container backend to Loom that runs each stage in an isolated per-stage container with the worktree topology preserved, network egress firewalled via iptables with an allowlist, and host credentials excluded by default — enabling the `bypassPermissions` permission mode safely. Replaces the hard-coded `acceptEdits` permission setting with a per-stage-type configurable `permission_mode` (native to auto to bypassPermissions) that is resolved at init time and spawn time, validated against the selected backend.

## Changes by Stage

### Stage 1 — Config & Runtime Identity (stage-1-config-and-identity)

**Status:** completed  
**Purpose:** Centralize .work/config.toml ownership, persist plan-level sandbox/execution
config across loom run reloads, add backend/runtime metadata to Session and
Stage. This is the foundation that addresses pressure-test findings #2, #3,
# 6, and #12.

Use parallel subagents and skills to maximize performance.

⛔ NEVER use Claude Code's auto-memory.
⛔ NEVER use loom knowledge update (reserved for knowledge stages).
Implementation stages use loom memory ONLY.

Reference: codex-PLAN-container-backend.md findings #2, #3, #6, #12.

SUBAGENT FILE ASSIGNMENTS:
  Subagent A — Centralized .work/config.toml API (loom-senior-software-engineer):
    Files Owned:
      src/fs/work_dir.rs (centralized API only — .work discovery helper
        at line 84 untouched here; Stage 3 owns that)
      Cargo.toml
    Files Read-Only:
      src/plan/schema/execution.rs (Subagent B's output)
    Tasks:
      1. Refactor fs/work_dir.rs to expose a single read/write/mutate API
         for .work/config.toml using toml_edit (preserves comments,
         formatting, unknown fields). API surface:
           pub fn read_config(work_dir: &Path) -> Result<Document>
           pub fn write_config(work_dir: &Path, doc: &Document) -> Result<()>
           pub fn read_project_execution(work_dir: &Path) -> Result<Option<ProjectExecutionConfig>>
           pub fn write_project_execution(work_dir: &Path, cfg: &ProjectExecutionConfig) -> Result<()>
           pub fn read_plan_sandbox(work_dir: &Path) -> Result<Option<SandboxConfig>>
           pub fn write_plan_sandbox(work_dir: &Path, sandbox: &SandboxConfig) -> Result<()>
           pub fn read_plan_execution(work_dir: &Path) -> Result<Option<PlanExecutionConfig>>
           pub fn write_plan_execution(work_dir: &Path, exec: &PlanExecutionConfig) -> Result<()>
      2. Add `cargo add toml_edit` if not present (`grep toml_edit Cargo.toml` first)
      3. Tests: round-trip preservation of comments and unknown keys.
      NOTE: Subagent A does NOT migrate any existing call sites. Subagent
      D owns ALL migrations (plan_setup.rs lines 21/142/300,
      plan_lifecycle.rs:68) so a single subagent has exclusive write
      access to those files.

  Subagent B — Schema layer for backend + plan-level container preferences (loom-senior-software-engineer):
    Files Owned:
      src/plan/schema/execution.rs (NEW)
      src/plan/schema/mod.rs (add `pub mod execution;`)
      src/plan/schema/types.rs (LoomConfig.execution + StageDefinition.execution fields ONLY — NOT PermissionMode, that's Stage 2)
      src/orchestrator/terminal/mod.rs (BackendType re-export ONLY — replace local enum)
    Files Read-Only:
      src/fs/work_dir.rs (for ProjectExecutionConfig type alignment)
    Tasks:
      1. Create execution.rs with:
         - enum BackendType { #[default] Native, Container }, kebab-case serde,
           Display, FromStr ("native" | "container") — SINGLE canonical definition
         - struct PlanExecutionConfig { #[serde(default, skip_serializing_if = "Option::is_none")] container: Option<PlanContainerConfig> }
           (NO `backend` field at plan level — project backend lives in .work/config.toml)
         - struct PlanContainerConfig { forward_credentials: Vec<String>, additional_mounts: Vec<String> }
         - struct StageExecutionConfig { backend: Option<BackendType> }
      2. Add `pub mod execution;` to plan/schema/mod.rs
      3. Add `pub execution: Option<PlanExecutionConfig>` to LoomConfig in
         plan/schema/types.rs (line 240 area). Field MUST be on LoomConfig
         (NOT a hypothetical Plan struct — finding #3 verified)
      4. Add `pub execution: Option<StageExecutionConfig>` to StageDefinition
      5. In orchestrator/terminal/mod.rs: REMOVE the local BackendType enum
         (lines 23-46) and its Display/FromStr impls; replace with:
           pub use crate::plan::schema::execution::BackendType;
         Update tests at lines 156-176 accordingly (move to execution.rs)
      6. Define ProjectExecutionConfig + ProjectContainerConfig structs co-located
         in execution.rs (or a sibling project_execution.rs) for use by Subagent A's
         fs/work_dir.rs API:
         - ProjectExecutionConfig { backend: BackendType, container: Option<ProjectContainerConfig> }
         - ProjectContainerConfig { runtime: String, fingerprint: String, image_digest: String, forward_credentials: Vec<String> }
      7. Tests: serde round-trip; kebab-case parsing; LoomConfig accepts
         execution: block

  Subagent C — Session and SessionType metadata (loom-senior-software-engineer):
    Files Owned:
      src/models/session/types.rs
      src/models/session/mod.rs (re-exports)
    Files Read-Only:
      src/plan/schema/execution.rs
    Tasks:
      1. Add SessionType::Knowledge variant; ensure all match arms updated
         across the codebase (run cargo build to find them)
      2. Add to Session struct (with serde defaults for back-compat parsing):
         pub backend: BackendType
         pub runtime: Option<String>
         pub container_name: Option<String>
         pub tracking_key: String
      3. Add helper:
         impl Session {
           pub fn derive_tracking_key(stage_id: &str, kind: SessionType) -> String {
             match kind {
               SessionType::Standard => format!("loom-{}", stage_id),
               SessionType::Merge => format!("loom-merge-{}", stage_id),
               SessionType::BaseConflict => format!("loom-base-conflict-{}", stage_id),
               SessionType::Knowledge => format!("loom-knowledge-{}", stage_id),
             }
           }
         }
      4. Update existing constructors to populate tracking_key + accept BackendType:
         Session::new(stage_id, backend) -> Self
         Session::new_merge(stage_id, backend) -> Self
         Session::new_base_conflict(stage_id, backend) -> Self
         Session::new_knowledge(stage_id, backend) -> Self  // NEW
      5. Tests: tracking_key derivation matrix; serde round-trip with new fields;
         old-format Session files (without new fields) parse correctly

  Subagent D — Stage propagation + plan-config persistence + centralized-API migrations (loom-senior-software-engineer):
    Files Owned:
      src/models/stage/types.rs
      src/models/stage/methods.rs (helpers if needed)
      src/commands/init/plan_setup.rs (ALL line ranges — 21, 142, 300)
      src/fs/plan_lifecycle.rs
      src/plan/graph/loader.rs
    Files Read-Only:
      src/plan/schema/execution.rs (Subagent B)
      src/fs/work_dir.rs (Subagent A)
    Tasks:
      1. Migrate commands/init/plan_setup.rs lines 21 and 142 from direct
         toml::Value writes to Subagent A's centralized fs::work_dir API.
      2. Migrate fs/plan_lifecycle.rs:68 from direct toml::Value writes to
         Subagent A's centralized API.
      3. Extend runtime Stage in models/stage/types.rs:251 with:
         pub execution_backend: Option<BackendType>
         plus helper fn execution_backend(&self) -> Option<BackendType>
      4. In commands/init/plan_setup.rs:300 (StageDefinition -> Stage materialization):
         copy stage_def.execution.as_ref().and_then(|e| e.backend) to runtime Stage
      5. Persist plan-level LoomConfig.sandbox to .work/config.toml under
         [plan_sandbox] via Subagent A's write_plan_sandbox API; persist
         LoomConfig.execution under [plan_execution] via write_plan_execution
         (called from init's plan_setup at line 300 area).
      6. Update plan/graph/loader.rs:62 + :69 so when stage files exist,
         plan-level sandbox/execution come from the persisted [plan_sandbox]
         and [plan_execution] blocks via Subagent A's read_* APIs — NOT
         SandboxConfig::default() (this is finding #2 — currently silently
         lost on loom run).
      7. Tests:
         - plan-level sandbox honored after .work/stages exists (regression
           test for finding #2)
         - stage-level execution.backend appears on runtime Stage
         - plan_lifecycle.rs round-trips through centralized API without
           losing comments / unknown keys

NO FILE OVERLAP between subagents confirmed. Subagent D has exclusive
write access to all migration sites; Subagent A defines the centralized
API D consumes.

After subagents complete, MAIN AGENT:

- cargo build (resolves any interface drift)
- cargo clippy --all-targets -- -D warnings
- cargo test
- Verify Session and Stage carry the new fields by inspecting a sample
  .work/sessions/*.md and .work/stages/*.md after a test init

MEMORY RECORDING (loom memory ONLY):
⛔ NEVER use Claude Code's auto-memory.
⛔ NEVER use loom knowledge update.

SUBAGENT MEMORY — every subagent prompt must include:
"Record mistakes, decisions, and surprises using loom memory. Do NOT
record procedural actions like 'read file' or 'ran tests'."

#### Files Changed

No changes recorded.

#### Key Decisions

- keep existing Session::new() no-arg signature; add new fields (backend, tracking_key, runtime, container_name) with serde defaults, plus derive_tracking_key helper and SessionType::Knowledge *(spec proposed Session::new(stage_id, backend) but 50+ test/src call sites use no-arg form; acceptance criteria only check rg patterns for tracking_key and SessionType::Knowledge — no signature constraint. Back-compat path is far less risky.)*

#### Notes

- gotcha: Adding a field to LoomConfig/StageDefinition/Stage/Session breaks ~15-20 explicit struct literals across src/ and tests/. Use Default::default() spread or migrate to ..X::default() pattern. Wrote /tmp/claude/patch_fields_v2.py to context-aware insert; future field-add stages should use the same approach.
- found: read_section originally went through toml_edit Item -> serde, which silently dropped nested sub-tables. Switched to re-parse the whole file with toml::Value then try_into on the section — preserves nested PlanContainerConfig / ProjectContainerConfig sub-tables. Lesson: toml_edit is for round-trip writes; toml is for typed reads.
- found: clippy's large_enum_variant fired when Session grew with new identity fields. Boxing the Session in AutoMergeResult::ConflictResolutionSpawned (and dereffing in merge_handler) is the lightweight fix; required matching Box<Session>/*session unbox at consumer.
- gotcha: cargo fmt is part of formatting verification but acceptance criteria don't list it. Run cargo fmt before commit even if not in acceptance — pre-commit hook may fail otherwise.

### Stage 2 — Permission-Mode Plumbing (stage-2-permission-mode-plumbing)

**Status:** completed  
**Purpose:** Replace hard-coded "acceptEdits" with a resolved permission_mode driven by
stage-type defaults, plan-level overrides, and stage-level overrides.
Wire validate_config into BOTH init time AND spawn time (finding #4).
Thread PermissionMode through every propagation site finding #5 enumerated
(HooksConfig, base worktree settings, hook generator). Use kebab-case YAML
for user-facing enum values per finding #5.

Use parallel subagents and skills to maximize performance.

⛔ NEVER use Claude Code's auto-memory.
⛔ NEVER use loom knowledge update.

Reference: codex-PLAN-container-backend.md findings #4, #5, #14
(env scrubbing piggybacks on hook + worktree settings work).

SUBAGENT FILE ASSIGNMENTS:
  Subagent A — PermissionMode + sandbox config + validate_config (loom-senior-software-engineer):
    Files Owned:
      src/plan/schema/types.rs (PermissionMode enum + SandboxConfig fields ONLY — NOT LoomConfig.execution which Stage 1 owns)
      src/sandbox/config.rs
      src/sandbox/settings.rs
    Files Read-Only:
      src/plan/schema/execution.rs
    Tasks:
      1. Add to types.rs:
         #[serde(rename_all = "kebab-case")]
         pub enum PermissionMode {
           Default, AcceptEdits, Auto, Plan, BypassPermissions
         }
         — YAML uses "accept-edits", "bypass-permissions" (per finding #5)
         impl PermissionMode {
           pub fn as_settings_value(self) -> &'static str {
             // returns Claude's camelCase: "default", "acceptEdits", "auto",
             // "plan", "bypassPermissions"
           }
         }
      2. Add to SandboxConfig and StageSandboxConfig:
         #[serde(default, skip_serializing_if = "Option::is_none")]
         pub permission_mode: Option<PermissionMode>
      3. Extend MergedSandboxConfig:
         pub permission_mode: PermissionMode  // resolved, never None
      4. Implement merge_config(plan, stage, stage_type):
         stage > plan > default_mode_for(stage_type)
      5. Implement default_mode_for(stage_type) -> PermissionMode:
         Standard | IntegrationVerify -> Auto
         Knowledge | KnowledgeDistill -> AcceptEdits
      6. Implement validate_config(merged: &MergedSandboxConfig, backend: BackendType) -> Result<()>:
         BypassPermissions + Native -> Err with anyhow context citing
         "use loom init <plan> --backend container to enable bypass mode"
      7. Add to settings.rs:
         pub fn apply_default_mode(settings: &mut serde_json::Value, mode: PermissionMode) -> Result<()>
         (writes to "permissions"."defaultMode")
         Wire generate_settings_json to call apply_default_mode
      8. Tests: matrix coverage for all 5 PermissionMode values × 2 backends;
         kebab-case parse round-trip; settings JSON output matches Claude's expected camelCase

  Subagent B — Hook generation propagation + env scrubbing (loom-software-engineer):
    Files Owned:
      src/hooks/config.rs
      src/hooks/generator.rs
    Files Read-Only:
      src/sandbox/{config,settings}.rs
    Tasks:
      1. Add field to HooksConfig in hooks/config.rs:98:
         pub permission_mode: PermissionMode
      2. Update all HooksConfig constructors to accept PermissionMode
      3. In hooks/generator.rs:42, replace hard-coded "acceptEdits" with
         config.permission_mode.as_settings_value()
      4. In hooks/generator.rs:79 (env block copy): per finding #14, scrub
         sensitive keys from copied env when targeting container backend.
         Use an allowlist of permitted env keys; drop anything else.
         Sensitive keys to ALWAYS strip: SSH_AUTH_SOCK, GH_TOKEN, GITHUB_TOKEN,
         AWS_*, GOOGLE_*, GCP_*, ANTHROPIC_API_KEY (Claude uses credentials.json,
         not env), MCP_* (MCP server env blocks).
      5. Tests: hooks settings reflect resolved mode; container target strips
         sensitive keys; native target preserves env unchanged

  Subagent C — Worktree settings propagation (loom-software-engineer):
    Files Owned:
      src/git/worktree/settings.rs
      src/git/worktree/operations.rs (line 87 call site only)
    Files Read-Only:
      src/sandbox/{config,settings}.rs
    Tasks:
      1. Update create_worktree_settings at git/worktree/settings.rs:338:
         add `mode: PermissionMode` parameter; replace hard-coded "acceptEdits"
         with mode.as_settings_value()
      2. Decision for git/worktree/settings.rs:72 (base settings written
         BEFORE sandbox merge): STOP writing defaultMode in base settings,
         rely solely on settings.local.json (which Subagent A's apply_default_mode
         populates with the resolved value). Add a code comment explaining
         why. This is the simpler choice from finding #5's two options.
      3. Update git/worktree/operations.rs:87 (the call site that writes
         base settings): drop the defaultMode write; pass through
         PermissionMode if other settings need it.
      4. In git/worktree/settings.rs:74 (env block copy when generating
         Claude settings): apply the same env-scrub allowlist Subagent B
         uses for hooks (per finding #14). Centralize the allowlist as
         a const so both modules reference one source of truth.
      5. Tests: worktree settings reflect resolved mode; base settings no
         longer write defaultMode; env scrubbing applies for container target

  Subagent D — validate_config call sites at init + spawn (loom-senior-software-engineer):
    Files Owned:
      src/commands/init/plan_setup.rs (ONLY validation hooks at lines 48 + 63 — NOT lines 21/142/300 owned by Stage 1)
      src/orchestrator/core/stage_executor.rs (validation at lines 244 + 347)
    Files Read-Only:
      src/sandbox/config.rs
      src/fs/work_dir.rs
    Tasks:
      1. In commands/init/plan_setup.rs around lines 48 + 63: for EVERY stage,
         read project backend from .work/config.toml via Stage 1's
         fs::work_dir::read_project_execution(). Then for each stage:
         let merged = sandbox::config::merge_config(
           &plan_sandbox, stage_def.sandbox.as_ref(), stage_type
         );
         sandbox::config::validate_config(&merged, project_backend)?;
         — bail with actionable error if validation fails
      2. In orchestrator/core/stage_executor.rs:244 (sandbox merge site)
         and :347 (settings write site): re-run validate_config as
         defense-in-depth. Bail if validation rejects (should never trigger
         if init passed, but catches .work/config.toml corruption or
         backend reconfig mid-run)
      3. Tests:
         - Native + bypassPermissions rejected at init for ANY stage in plan
         - Native + bypassPermissions rejected at spawn even if .work was
           hand-edited to remove the project-backend marker
         - Container + bypassPermissions accepted both places
         - Standard stage with no permission_mode resolves to "auto" at spawn

NO FILE OVERLAP between subagents confirmed.

After subagents complete, MAIN AGENT:

- cargo build (resolve any interface drift)
- cargo clippy --all-targets -- -D warnings
- cargo test
- rg "acceptEdits" src/ — should yield ONLY apply_default_mode's match arm
  (the ONE site that writes Claude's camelCase value)

MEMORY RECORDING (loom memory ONLY).
SUBAGENT MEMORY — every subagent prompt must include the standard memory clause.

#### Files Changed

No changes recorded.

#### Key Decisions

- skip writing defaultMode in base settings.json; rely on settings.local.json via apply_default_mode *(finding #5 option 2: base settings is written BEFORE sandbox merge, so it has no resolved mode. Putting defaultMode there would race the per-stage value. settings.local.json is read by Claude Code with higher precedence anyway.)*

#### Notes

- mistake: tried to wrap PermissionMode directly in plan-level SandboxConfig (non-Option), but plan/stage merge needs precedence layering. Fix: plan-level and stage-level both use Option<PermissionMode>; MergedSandboxConfig holds resolved (non-Option) PermissionMode. Stage > plan > default_mode_for(stage_type).
- gotcha: orchestrator::terminal::BackendType is a re-export, not a separate enum. Adding 'use crate::plan::schema::BackendType' then passing self.config.backend_type compiles cleanly because they resolve to the same type. Don't waste time adding From/Into shims.
- found: BackendType is canonical in plan::schema::execution and re-exported from orchestrator::terminal — same type, no conversion needed. self.config.backend_type (from terminal module) passes directly into sandbox::validate_config and git operations.

### Stage 3 — Container Backend, Dispatcher, Liveness, Spawn-Path Wiring (stage-3-container-backend)

**Status:** completed  
**Purpose:** Implement ContainerBackend with the /repo mount topology (finding #1),
wire it through every spawn path the original plan missed (findings #9 +
# 10), replace direct PID liveness checks with a backend-aware service
(finding #7), and route loom sessions kill / clean / stop through
persisted session backend metadata (finding #8). Hook generation and
Claude settings become backend-aware here so .work discovery and hooks
work inside the container before image/firewall complexity is added.

Use parallel subagents and skills to maximize performance.
Apply loom-rust, loom-docker skills as appropriate.

⛔ NEVER use Claude Code's auto-memory.
⛔ NEVER use loom knowledge update.

Reference: codex-PLAN-container-backend.md findings #1, #7, #8, #9, #10,
# 13, #14.

CONTAINER TOPOLOGY (decided in knowledge-bootstrap, repeated here):

- Mount entire host repo root at /repo (rw) — preserves git worktree
  metadata, the relative .work symlink, and host-absolute hook references
- Standard stage cwd: /repo/.worktrees/{stage_id}
- Merge / base-conflict / knowledge cwd: /repo
- LOOM_WORK_DIR=/repo/.work — CLI .work discovery prefers this when set
- Bind mounts:
  - <host repo root> rw -> /repo
  - <host>/.work/network/allowed_domains.txt ro -> /etc/loom/network/allowed_domains.txt
    (finding #16 — outside agent-writable mount; full impl in stage 4)
  - ~/.claude/.credentials.json ro -> /home/loom/.claude/.credentials.json
    ONLY if forward_credentials contains "claude" (default per finding #13)
  - ~/.claude/hooks/loom ro -> /home/loom/.claude/hooks/loom
- Env strip: SSH_AUTH_SOCK, GH_TOKEN, GITHUB_TOKEN, GIT_ASKPASS=/bin/false
- Env populate: LOOM_SESSION_ID, LOOM_STAGE_ID, LOOM_WORK_DIR=/repo/.work,
  LOOM_WORKTREE_PATH=/repo/.worktrees/{stage} (or /repo for merge variants),
  CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1

SUBAGENT FILE ASSIGNMENTS:
  Subagent A — ContainerBackend implementation (loom-senior-software-engineer):
    Files Owned:
      src/orchestrator/terminal/container/mod.rs (NEW)
      src/orchestrator/terminal/container/runtime.rs (NEW)
      src/orchestrator/terminal/container/lifecycle.rs (NEW)
      src/orchestrator/terminal/container/network.rs (STUB ONLY — full impl in stage 4)
    Files Read-Only:
      src/orchestrator/terminal/{native/mod.rs,native/pid_tracking.rs}
    Tasks:
      1. runtime.rs:
         - enum Runtime { Docker, Podman, AppleContainer } with binary() and
           user_args() (Podman -> --userns=keep-id; Docker -> --user=uid:gid
           via libc::getuid()/getgid(); Apple Container -> [])
         - detect_runtime(preference: &str) -> Result<Runtime> accepting
           "docker" | "podman" | "container" | "auto"
         - macOS auto-priority: AppleContainer > Podman > Docker
         - Linux auto-priority: Podman > Docker
         - Apple Container detection: CHECK /usr/local/bin/container EXISTS
           AND `container --version` returns Apple-signature output
           (NOT just `which container` — collision risk)
         - cfg(target_os = "macos") guards
         - Hard-fail with OS-specific install instructions if no runtime found
      2. lifecycle.rs:
         - build_run_args(name, image_ref, mounts, env_set, env_strip, network, runtime):
           "<runtime> run -d --rm --name=... --cap-drop=ALL --cap-add=NET_ADMIN
            --cap-add=NET_RAW --network=loom-net-{stage_id}"
         - Bind mounts use --mount=type=bind,source=...,target=... form
           (NEVER -v; Apple Container portability)
         - Apply runtime.user_args()
         - Env strip via -e VAR= for each in env_strip; env populate via -e VAR=value
         - Append image, then wrapper script path inside container
           (/repo/.work/wrappers/{stage_id}.sh)
      3. network.rs (STUB):
         pub fn ensure_network(_: &Runtime, *: &str) -> Result<()> { Ok(()) }
         pub fn write_allowlist(*: &Path, *: &NetworkConfig) -> Result<PathBuf> {
           bail!("network::write_allowlist not implemented until stage 4")
         }
         pub fn remove_network(*: &Runtime,_: &str) -> Result<()> { Ok(()) }
         — Full implementation in stage 4 Subagent C
      4. mod.rs ContainerBackend struct:
         pub struct ContainerBackend {
           runtime: Runtime,
           work_dir: PathBuf,
           image_ref: String,
         }
         ContainerBackend::new(work_dir):
         - detect_runtime("auto")
         - resolve image_ref via fs::work_dir::read_project_execution()
         - error if no pinned digest with: "expected execution.container.image_digest;
           run loom init --backend container"
      5. Implement TerminalBackend trait — all four spawn variants:
         - tracking_key from session.tracking_key (Stage 1's helper)
         - container_name = session.tracking_key.clone()
         - network::ensure_network(runtime, stage_id)
         - Generate wrapper script via Subagent B's WrapperPaths with
           container-relative paths (/repo/.work, /repo/.worktrees/{stage}
           or /repo for merge variants)
         - <runtime> run -d returns immediately
         - Poll <runtime> inspect -f '{{.State.Running}}' until true (10s timeout)
         - For Docker/Podman: write inspect Pid to /repo/.work/pids/{stage_id}.pid;
           skip for AppleContainer
         - session.set_pid(pid) for Docker/Podman, session.container_name = Some(name),
           session.runtime = Some(runtime.binary().to_string()),
           session.try_mark_running()
         - If !no_attach: spawn host terminal running
           `<runtime> exec -it <name> /bin/bash -lc 'tail -f /repo/.work/sessions/{session_id}.log'`
      6. kill_session: <runtime> rm -f session.container_name; network::remove_network
      7. is_session_alive: parse <runtime> inspect -f '{{.State.Running}}' container_name
         — MUST NOT use kill -0 against host PID
      8. backend_type: BackendType::Container
      9. Tests: build_run_args snapshot per (runtime, mount-set, env-strip variant);
         detect_runtime branches with mocked PATH

  Subagent B — Dispatcher, factory, WrapperPaths, liveness service (loom-senior-software-engineer):
    Files Owned:
      src/orchestrator/terminal/dispatcher.rs (NEW)
      src/orchestrator/terminal/mod.rs (factory create_backend ONLY — re-export from Stage 1 unchanged)
      src/orchestrator/terminal/native/pid_tracking.rs (additive WrapperPaths)
      src/orchestrator/liveness.rs (NEW)
      src/orchestrator/mod.rs (add `pub mod liveness;`)
    Files Read-Only:
      src/orchestrator/terminal/container/**
      src/models/session/types.rs
      src/plan/schema/execution.rs
    Tasks:
      1. dispatcher.rs:
         pub struct BackendDispatcher {
           native: Option<Box<dyn TerminalBackend>>,
           container: Option<Box<dyn TerminalBackend>>,
         }
         pub struct BackendNeeds { pub native: bool, pub container: bool }
         impl BackendDispatcher {
           pub fn for_plan(project: BackendType, needs: BackendNeeds, work_dir: &Path) -> Result<Self>
           pub fn for_stage(&self, backend: BackendType) -> &dyn TerminalBackend
           pub fn for_session(&self, session: &Session) -> &dyn TerminalBackend
         }
         pub fn resolve_stage_backend(project: BackendType, stage_override: Option<BackendType>) -> Result<BackendType>
         — narrowing only: container project + native stage = Ok;
           native project + container stage = Err with actionable message
      2. terminal/mod.rs create_backend factory: handle BackendType::Container
      3. native/pid_tracking.rs WrapperPaths struct:
         pub struct WrapperPaths {
           pub work_dir_in_container: PathBuf,
           pub workspace_in_container: Option<PathBuf>,
         }
         — Add create_wrapper_script_with_paths(work_dir, stage_id, session_id, claude_cmd, paths)
         — Original create_wrapper_script delegates with host-path defaults
      4. liveness.rs:
         pub struct LivenessService { dispatcher: Arc<BackendDispatcher> }
         impl LivenessService {
           pub fn new(dispatcher: Arc<BackendDispatcher>) -> Self
           pub fn is_alive(&self, session: &Session) -> Result<bool> {
             self.dispatcher.for_session(session).is_session_alive(session)
           }
         }

  Subagent C — Wire dispatcher into ALL spawn paths + run preflight + sessions/clean/stop (loom-senior-software-engineer):
    Files Owned:
      src/orchestrator/core/orchestrator.rs
      src/orchestrator/core/stage_executor.rs
      src/orchestrator/monitor/handlers.rs
      src/orchestrator/monitor/detection.rs
      src/commands/run/foreground.rs
      src/daemon/server/orchestrator.rs
      src/commands/stage/merge_resolver.rs
      src/orchestrator/continuation/mod.rs
      src/orchestrator/auto_merge.rs
      src/commands/sessions.rs
      src/commands/clean.rs
      src/commands/stop.rs
    Files Read-Only:
      src/orchestrator/terminal/dispatcher.rs
      src/orchestrator/liveness.rs
      src/fs/work_dir.rs
    Tasks:
      1. Replace Box<dyn TerminalBackend> with BackendDispatcher in
         OrchestratorConfig and Orchestrator struct (orchestrator.rs).
         Compute BackendNeeds from loaded plan (which stages opt to native
         vs container)
      2. In stage_executor.rs: before each spawn_*, call
         dispatcher.for_stage(resolve_stage_backend(project, stage.execution_backend()))
         then session.backend = resolved_backend before passing to backend.spawn_*
      3. SHARED RUN PREFLIGHT (new helper, can live in orchestrator/preflight.rs
         or commands/run/common.rs):
         - read project_backend from .work/config.toml via Stage 1 API
         - reject image_digest = "pending" with actionable error
         - validate every stage's resolved backend (resolve_stage_backend matrix)
         - return BackendDispatcher::for_plan(...)
      4. commands/run/foreground.rs:60 — replace hardcoded BackendType::Native
         with shared run preflight result
      5. daemon/server/orchestrator.rs:78 — same
      6. commands/stage/merge_resolver.rs:74 — replace NativeBackend::new
         with dispatcher lookup (read project from .work/config.toml)
      7. orchestrator/continuation/mod.rs:33 + :94 — read backend from
         .work/config.toml instead of defaulting to Native
      8. orchestrator/auto_merge.rs:68 — accept BackendDispatcher (or use
         shared lookup) instead of single backend reference
      9. monitor/handlers.rs:39 + monitor/detection.rs:119 + :360 —
         replace direct .work/pids reads + kill -0 with
         LivenessService::is_alive(&session) (uses session.backend to route)
      10. commands/sessions.rs:168 — route via dispatcher.for_session(&session).kill_session(&session)
          using session.backend; do NOT infer backend from PID presence
      11. commands/clean.rs:246 — in --sessions mode, also enumerate and
          clean orphan loom-*containers and loom-net-* networks
      12. commands/stop.rs:13 — decide: stop daemon only, or also terminate
          active containers? Recommend: also terminate (iterate live container
          sessions and kill_session each). Document choice.

  Subagent D — Backend-aware hook + Claude settings generation (loom-senior-software-engineer):
    Files Owned:
      src/hooks/config.rs (add backend field)
      src/hooks/generator.rs (container-stable paths + LOOM_WORK_DIR)
      src/fs/permissions/hooks.rs
      src/git/worktree/settings.rs (per-backend Claude settings)
      src/commands/common/mod.rs
      src/fs/work_dir.rs (.work discovery helper at line 84 ONLY — Stage 1 owns the schema reads)
    Files Read-Only:
      src/orchestrator/terminal/dispatcher.rs
      src/plan/schema/execution.rs
    Tasks:
      1. Add field to HooksConfig: pub backend: BackendType
         (Stage 2 Subagent B added permission_mode; this adds backend)
      2. fs/permissions/hooks.rs:14 + hooks/config.rs:126: when backend is
         Container, emit hook command paths as container-stable paths
         (/home/loom/.claude/hooks/loom/<hook>.sh) NOT host-absolute paths.
         Native keeps host-absolute.
      3. hooks/generator.rs:86: when backend is Container, write
         LOOM_WORK_DIR=/repo/.work (NOT host path)
      4. git/worktree/settings.rs: generate Claude settings.local.json
         per backend. For container, use an ALLOWLIST of permitted env keys
         when copying user settings (per finding #14). Drop MCP env blocks
         and any sensitive keys not on the allowlist. Document the allowlist
         in a code comment with rationale.
      5. commands/common/mod.rs:18 + fs/work_dir.rs:84: prefer validated
         LOOM_WORK_DIR env var over upward filesystem walk when set.
         Validation: must exist, must be a directory, must contain
         a recognizable .work marker (e.g. config.toml).

NO FILE OVERLAP between subagents confirmed. C and D both touched files
Stage 1 also touched, but Stage 1 has fully merged at this point so they
are stable inputs.

After subagents complete, MAIN AGENT:

- cargo build (resolve interface drift between A/B/C/D)
- cargo clippy --all-targets -- -D warnings
- cargo test (live container tests gated on LOOM_TEST_CONTAINERS=1; skipped here)
- Verify monitor uses LivenessService (rg "kill\(" src/orchestrator/monitor/
  should yield NO matches)

MEMORY RECORDING (loom memory ONLY).
SUBAGENT MEMORY — every subagent prompt must include the standard memory clause.

#### Files Changed

No changes recorded.

#### Key Decisions

- introduced LivenessService::fixed_for_tests(bool) for monitor tests *(monitor crash-detection tests still need to assert behavior when PID is dead, but Handlers::check_session_alive now routes through LivenessService instead of probing host PIDs. Test stub lets those tests exercise the same crash path without spinning up a full BackendDispatcher.)*

#### Notes

- found: NativeBackend Apple Container detection requires both /usr/local/bin/container existing AND 'container --version' returning Apple-signature output. Plain 'which container' would collide with unrelated tools named container. Detection rule lives in container/runtime.rs::is_apple_container.
- gotcha: ExecutionGraph.StageNode does NOT carry execution_backend — to compute BackendNeeds you have to load each stage file via verify::transitions::load_stage. Easy trap if adding similar per-stage routing later.
- decision: gated LOOM_WORK_DIR env override behind a path prefix check (only paths starting with /repo/ are honoured). Reason: cargo test inherits LOOM_WORK_DIR from the parent loom session and was hijacking tests that called WorkDir::new(temp_dir). Container topology guarantees /repo/.work so the gate is safe.

### Stage 4 — Image Cache, Firewall Hardening, Init Reconfigure, CLI (stage-4-image-cache-and-firewall)

**Status:** completed  
**Purpose:** Implement the global image cache (with correct fingerprinting, embedded
resources, complete cache key per findings #18-#21), the production
firewall (with capability drop and root-owned allowlist per findings
# 15-#17), the init reconfigure path (per finding #11), and the CLI
surfaces (loom container group + dynamic completions per #22-#23).

Use parallel subagents and skills to maximize performance.
Apply loom-rust, loom-docker skills as appropriate.

⛔ NEVER use Claude Code's auto-memory.
⛔ NEVER use loom knowledge update.

Reference: codex-PLAN-container-backend.md findings #11, #15, #16, #17,
# 18, #19, #20, #21, #22, #23.

BEFORE FANNING OUT SUBAGENTS, MAIN AGENT must create empty placeholder
files so Subagent A's fingerprint.rs (which uses include_str! for
Dockerfile.tmpl, firewall.sh, entrypoint.sh) can compile in parallel with
Subagents B and C filling those files in:

- touch loom/resources/Dockerfile.tmpl
- touch loom/resources/firewall.sh
- touch loom/resources/entrypoint.sh
These will be overwritten by Subagents B (Dockerfile) and C
(firewall + entrypoint). Final cargo build sees the real contents.

SUBAGENT FILE ASSIGNMENTS:
  Subagent A — Fingerprinting with nested-manifest scan (loom-software-engineer):
    Files Owned:
      src/orchestrator/terminal/container/fingerprint.rs (NEW)
      src/language.rs
    Files Read-Only:
      resources/Dockerfile.tmpl (Subagent B's output — only for hashing)
      resources/firewall.sh (Subagent C's output — only for hashing)
    Tasks:
      1. Add DetectedLanguage::canonical_name(&self) -> &'static str returning
         lowercase short names: rust, typescript, python, go.
         IMPORTANT: distinct from skill_name() so Go does NOT map to "golang"
         (per finding #18)
      2. Extend detect_project_languages to optionally accept additional scan
         roots (or have compute_fingerprint invoke it once per root)
      3. compute_fingerprint(project_root: &Path, plan_working_dirs: &[PathBuf]) -> String:
         - Detect languages by scanning project_root AND each plan_working_dirs[i]
         - Sort + dedup by canonical_name()
         - Join with "-", fallback "base"
         - Format: "<langs>-<combined-hash[:8]>" where combined-hash is SHA-256 of:
           *include_str! Dockerfile.tmpl
           * include_str! firewall.sh (per finding #19)
           * The rendered Handlebars feature switches for the detected language set
         (Note: sha2 + hex are already in Cargo.toml — do NOT re-add)
      4. Tests:
         - TempDir with synthetic root + nested Cargo.toml at subdir/Cargo.toml,
           compute_fingerprint([root], [subdir]) detects rust
         - Same input -> same fingerprint
         - Flipping firewall.sh content changes fingerprint (regression for #19)
         - Flipping Dockerfile.tmpl changes fingerprint
         - Go canonical name is "go" (regression for #18)

  Subagent B — Image cache + embedded resources + cache directory (loom-senior-software-engineer):
    Files Owned:
      src/orchestrator/terminal/container/image.rs (NEW)
      src/orchestrator/terminal/container/resources.rs (NEW)
      resources/Dockerfile.tmpl (NEW)
    Files Read-Only:
      src/orchestrator/terminal/container/{runtime,fingerprint}.rs
      resources/firewall.sh (Subagent C's output — for COPY into Dockerfile)
    Tasks:
      1. resources.rs (per finding #21 — embed via include_str! like
         fs/permissions/constants.rs:3):
         pub const DOCKERFILE_TEMPLATE: &str = include_str!("../../../../resources/Dockerfile.tmpl");
         pub const FIREWALL_SCRIPT: &str = include_str!("../../../../resources/firewall.sh");
         pub const ENTRYPOINT_SCRIPT: &str = include_str!("../../../../resources/entrypoint.sh");
      2. resources/Dockerfile.tmpl per the spec in stage-4 plan section:
         - Base: mcr.microsoft.com/devcontainers/base:ubuntu-24.04
         - ARGs: USERNAME=loom, USER_UID=1000, USER_GID=$USER_UID
         - apt-get install: sudo iptables ipset dnsutils ripgrep fd-find git curl
           ca-certificates e2fsprogs gosu (gosu for capability drop)
         - Create loom user/group at UID 1000
         - Install Node.js 22 + @anthropic-ai/claude-code globally
         - Conditional toolchains via {{#if has_rust}} / has_typescript / has_python / has_go
         - COPY firewall.sh -> /usr/local/bin/loom-firewall.sh; chmod +x;
           chattr +i || true
         - Sudoers: loom ALL=(ALL) NOPASSWD: /usr/local/bin/loom-firewall.sh
         - WORKDIR /repo
         - NO `USER loom` — container starts as root for firewall setup
         - COPY entrypoint.sh; ENTRYPOINT ["/usr/local/bin/loom-entrypoint.sh"]
      3. image.rs:
         - cache_dir() -> Result<PathBuf>:
           * env-override priority: $LOOM_CACHE_DIR if set
           *Linux default: ${XDG_CACHE_HOME:-$HOME/.cache}/loom/images
           * macOS default: $HOME/Library/Caches/loom/images
           (per finding #20 — LOOM_CACHE_DIR is the IMAGE cache root,
            containing per-fingerprint subdirectories)
         - ensure_image(fingerprint, runtime, force) -> Result<String> (digest):
           * Cache hit: read image.digest, verify with `<runtime> image inspect`,
             return digest
           * Cache miss (or force): write embedded constants (Dockerfile rendered
             from template, firewall.sh, entrypoint.sh) to
             cache_dir()/<fingerprint>/, run `<runtime> build -t loom/base:<fingerprint>
             --progress=plain .`, capture digest via `<runtime> inspect`,
             write image.digest + built_at
           * Stream native build output to stdout (no spinner)
         - Render Dockerfile from DOCKERFILE_TEMPLATE constant via simple
           substitution (or handlebars). Decide based on complexity. If
           handlebars needed, `cargo add handlebars` (Cargo.toml owned by
           Subagent D — coordinate)
      4. Tests:
         - cache_dir() with $LOOM_CACHE_DIR override (use #[serial] for env mutation)
         - cache_dir() Linux default (HOME mocked)
         - cache_dir() macOS default (cfg-gated)
         - cache hit/miss with mocked runtime command
         - ensure_image writes embedded contents to cache subdirectory

  Subagent C — Firewall script + network module + capability boundary (loom-senior-software-engineer):
    Files Owned:
      src/orchestrator/terminal/container/network.rs (replace stub from stage 3)
      src/orchestrator/terminal/container/mod.rs (extend spawn_session for network mounts)
      src/orchestrator/terminal/container/lifecycle.rs (extend run args for new mounts)
      resources/firewall.sh (NEW)
      resources/entrypoint.sh (NEW)
    Files Read-Only:
      src/sandbox/config.rs (NetworkConfig type)
    Tasks:
      1. resources/firewall.sh:
         #!/bin/bash
         set -euo pipefail
         LOCK=/var/run/loom-firewall.lock
         # Idempotency lockfile so unprivileged agent cannot rerun (per finding #16)
         if [ -e "$LOCK" ]; then exit 0; fi
         touch "$LOCK"
         chmod 0400 "$LOCK"

         POLICY=/etc/loom/network/allowed_domains.txt
         ALWAYS=(api.anthropic.com registry.npmjs.org)

         # Deny IPv6 entirely (per finding #17)
         ip6tables -P OUTPUT DROP
         ip6tables -P FORWARD DROP
         ip6tables -P INPUT DROP

         # IPv4 default deny outbound, allow loopback
         iptables -P OUTPUT DROP
         iptables -P FORWARD DROP
         iptables -A OUTPUT -o lo -j ACCEPT

         # Block cloud metadata IP (per finding #17)
         iptables -A OUTPUT -d 169.254.169.254 -j REJECT --reject-with icmp-net-unreachable

         # Block RFC1918 except loopback (per finding #17)
         iptables -A OUTPUT -d 10.0.0.0/8 -j REJECT --reject-with icmp-net-unreachable
         iptables -A OUTPUT -d 172.16.0.0/12 -j REJECT --reject-with icmp-net-unreachable
         iptables -A OUTPUT -d 192.168.0.0/16 -j REJECT --reject-with icmp-net-unreachable

         # DNS restriction: only to /etc/resolv.conf nameservers (per finding #17)
         # NOT to public DoH resolvers unless allowlisted
         for ns in $(awk '/^nameserver/ {print $2}' /etc/resolv.conf); do
           iptables -A OUTPUT -p udp -d "$ns" --dport 53 -j ACCEPT
           iptables -A OUTPUT -p tcp -d "$ns" --dport 53 -j ACCEPT
         done

         # Build allowlist
         ipset create loom-allowed hash:ip family inet -exist
         ipset flush loom-allowed
         for domain in "${ALWAYS[@]}" $(grep -v '^#' "$POLICY" 2>/dev/null || true); do
           for ip in $(dig +short "$domain" A | grep -E '^[0-9.]+$'); do
             ipset add loom-allowed "$ip" 2>/dev/null || true
           done
         done

         # Allow allowlisted ips
         iptables -A OUTPUT -m set --match-set loom-allowed dst -j ACCEPT

         # Reject everything else
         iptables -A OUTPUT -j REJECT --reject-with icmp-net-unreachable
      2. resources/entrypoint.sh (per finding #15):
         #!/bin/bash
         set -euo pipefail
         # Run firewall as root, then drop privileges
         /usr/local/bin/loom-firewall.sh
         exec gosu loom "$@"
      3. network.rs (REPLACE stage-3 stub):
         pub fn write_allowlist(work_dir: &Path, network: &NetworkConfig) -> Result<PathBuf>:
         - Writes to work_dir.join("network/allowed_domains.txt") on the HOST
           (this is what gets mounted ro at /etc/loom/network/allowed_domains.txt
            inside the container — agent cannot modify, per finding #16)
         - Header comment:
           "# Generated by loom; do not edit by hand.\n
            # Configure via plan: loom.sandbox.network.allowed_domains\n"
         - Writes allowed_domains then additional_domains (one per line)
         - Returns the path
         pub fn ensure_network(runtime: &Runtime, stage_id: &str) -> Result<()>:
         - Idempotent `<runtime> network create loom-net-{stage_id}`
         - Ignore "already exists" errors
         pub fn remove_network(runtime: &Runtime, stage_id: &str) -> Result<()>:
         - Idempotent cleanup
      4. Modify ContainerBackend::spawn_session (and merge/knowledge variants)
         in container/mod.rs to:
         - Call write_allowlist(<host repo>/.work, &network_config)
         - Add new RO mount: <host>/.work/network/allowed_domains.txt ->
           /etc/loom/network/allowed_domains.txt
         - Call ensure_network before launch
      5. Modify kill_session in container/mod.rs to call remove_network
         after `rm -f` (already wired in stage 3 — verify it calls the
         real implementation now)
      6. Tests:
         - bash -n firewall.sh (syntax check via `cargo test` running shell command)
         - bash -n entrypoint.sh
         - write_allowlist with empty / sparse / large NetworkConfig
         - ensure_network is idempotent (call twice -> no error)

  Subagent D — Init reconfigure + loom container subcommands + completions (loom-senior-software-engineer):
    Files Owned:
      src/commands/init/execute.rs (reconfigure path)
      src/fs/work_dir.rs (open_or_initialize helper at line 113 area ONLY — Stage 1's centralization remains intact)
      src/cli/dispatch.rs
      src/cli/types.rs
      src/commands/container/mod.rs (NEW)
      src/commands/container/build.rs (NEW)
      src/commands/container/rebuild.rs (NEW)
      src/commands/container/doctor.rs (NEW)
      src/commands/container/shell.rs (NEW)
      src/commands/mod.rs (per finding #23 — export container module)
      src/completions/dynamic/commands.rs (per finding #23)
      src/main.rs (CLI registration)
      Cargo.toml (only Cargo.toml owner this stage — per finding #22)
    Files Read-Only:
      src/orchestrator/terminal/container/{fingerprint,image,runtime}.rs
      src/orchestrator/terminal/dispatcher.rs
    Tasks:
      1. Init reconfigure path in commands/init/execute.rs:89:
         - When .work/ exists AND no --backend flag: PRESERVE existing
           [execution] block, re-parse plan to refresh stage definitions,
           do NOT delete .work
         - When .work/ exists AND --backend X: rewrite [execution] via
           Stage 1's API. If switching to container: fingerprint + ensure_image.
           If switching to native: drop digest.
         - InitGuard MUST never delete pre-existing .work (per finding #11)
      2. fs/work_dir.rs:113: replace `bail!(".work directory already exists")`
         with WorkDir::open_or_initialize that initializes if absent,
         returns existing if present
      3. Add --backend native|container and --no-build flags to loom init
         in cli/types.rs
      4. Wire dispatch in cli/dispatch.rs
      5. --no-build writes image_digest = "pending"; the SHARED RUN PREFLIGHT
         (added in stage 3 Subagent C) refuses to start with "pending"
      6. Print init summary: backend, runtime, fingerprint, cached/built, elapsed
      7. loom container command group:
         - build [--fingerprint <fp>]
         - rebuild [--fingerprint <fp>] [--all]
         - doctor: runtime, version, current fingerprint, pinned digest,
           image presence, cache size, all cached fingerprints,
           RUNTIME-SPECIFIC CAPABILITY CHECKS (rootless Podman / Apple
           Container limitations per finding #15) — warn if rootless
           Podman detected and explain capability constraints
         - shell <stage-id>: interactive `<runtime> exec -it <session.container_name> /bin/bash`
         - NO `loom container clean` (footgun)
      8. commands/mod.rs: add `pub mod container;` (per finding #23)
      9. completions/dynamic/commands.rs:6: add loom container subcommand
         group and loom init --backend|--no-build flags to dynamic
         completion tables (per finding #23)
      10. has_subcommands() in cli/types.rs:29 area: recognize "container"
          as having subcommands (per finding #23)
      11. Cargo dependency hygiene (per finding #22):
          - sha2, hex, which, libc are ALREADY in Cargo.toml — do NOT re-add
          - toml_edit was added in Stage 1 if not already — verify, do NOT re-add
          - handlebars: add ONLY if Subagent B reports needing it
            (`cargo add handlebars` after coordinating with B)

NO FILE OVERLAP between subagents confirmed. C extends container/mod.rs
and container/lifecycle.rs from Stage 3 with network plumbing — Stage 3
merged first so those files are stable inputs.

After subagents complete, MAIN AGENT:

- cargo build (resolve any drift)
- cargo clippy --all-targets -- -D warnings
- cargo test
- bash -n loom/resources/firewall.sh && bash -n loom/resources/entrypoint.sh
- ./target/debug/loom container --help shows build/rebuild/doctor/shell
- ./target/debug/loom init --help shows --backend / --no-build

MEMORY RECORDING (loom memory ONLY).
SUBAGENT MEMORY — every subagent prompt must include the standard memory clause.

#### Files Changed

No changes recorded.

#### Key Decisions

- factored compute_fingerprint_inner for testability *(include_str\! constants can't be mutated at test time, so the inner fn takes the contents as args, public fn calls it with the const inputs)*
- render Dockerfile via handlebars, fingerprint as the only source of language flags *(fingerprint already encodes detected langs (rust-typescript-<hash>); re-detecting at build time would risk drift between cache key and what's baked into the image)*
- store NetworkConfig on ContainerBackend at construction *(spawn paths run frequently; reading config.toml every spawn would add latency and a sandbox round trip. Cached value is read once in new() and from_project().)*
- open_or_initialize on WorkDir vs replacing initialize *(existing callers may rely on initialize() bailing; safer to add a new method (per finding #11))*
- split stage-4 commit into 5 logical commits *(fingerprint, image cache, firewall+network, init reconfigure, container CLI — each independently reviewable per CLAUDE.md rule 4)*

#### Notes

- mistake: instructed subagents to use 5 dotdot-segments for include_str path from container subdirectory to loom/resources. Correct is 4. Compiler suggests fix. Prevention: include_str at src/foo/bar/file.rs needs 4 hops up to loom/.
- deviation: find_sessions_for_stage returns Vec<String> (session IDs), not Session objects. shell.rs scans .work/sessions/*.md directly via parse_from_markdown to filter by stage_id AND container_name presence (must be a live container session)
- blocker: cargo build/check blocked by sandbox — ~/.cargo/registry is read-only, cannot fetch handlebars 6.4.0 even with Cargo.lock pinned. main agent must run build/tests outside sandbox to verify Subagent D output
- found: ContainerBackend already wires ensure_network in spawn_common; init reconfigure only needs to write [project_execution]
- gotcha: cargo build in sandbox fails when a new dependency (handlebars, added by subagent B) is not yet in local registry cache — needs network + write access to ~/.cargo. Main agent must run cargo build with sandbox disabled to verify.
- found: spawn_common in container/mod.rs already calls ensure_network from stage 3 — stage 4 only needs to add write_allowlist call (before ensure_network) and read plan_sandbox on construction
- build blocker: handlebars = "6.4.0" added to Cargo.toml (by Subagent D) requires network download. Sandbox blocks crate download. Main agent must run cargo build with network access (loom run or unsandboxed environment) to verify full build. My files (fingerprint.rs, language.rs) use only already-cached crates: sha2, hex, tempfile.
- subagent B: include_str depth for resources.rs in container module is five dotdots back to loom/, then resources/X. Five levels deep under loom (container, terminal, orchestrator, src, loom)

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final integration verification — runs after all feature stages complete.

Use parallel subagents and skills to maximize performance.

CRITICAL: Verify FUNCTIONAL INTEGRATION, not just tests passing. Container
backend must be ACTUALLY wired (loom run reads .work/config.toml, dispatcher
routes per session, monitor uses LivenessService, sessions kill works for
container sessions, firewall denies non-allowlisted egress).

⛔ NEVER use Claude Code's auto-memory.
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):

- Read plan file at doc/plans/IN_PROGRESS-PLAN-container-backend.md
  (check .work/config.toml source_path)
- Read codex-PLAN-container-backend.md (the pressure-test) to confirm
  all 23 findings have been addressed
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for architecture context

⛔ ZERO TOLERANCE: ALL compiler warnings, linter errors, test failures must
be FIXED. Nothing is "pre-existing." If genuinely blocked, report as a
blocker with a clear reason.

BUILD & TEST:

1. cargo build
2. cargo build --release
3. cargo test
4. cargo clippy --all-targets -- -D warnings
5. cargo fmt --check (informational; if it fails, run cargo fmt and commit
   separately rather than blocking)

CODE REVIEW (MANDATORY — spawn parallel loom-code-reviewer subagents in ONE message):
6. Spawn:

- Security review (loom-code-reviewer + /loom-security-audit):
  - Container escape vectors
  - Env var leakage (verify SSH_AUTH_SOCK / GH_TOKEN / GITHUB_TOKEN /
       AWS_*/ MCP_* are stripped from container env AND from copied settings)
  - Credential mount only when forward_credentials opts in (default ["claude"])
  - Firewall script integrity (chattr +i, sudoers scope, lockfile)
  - Allowlist mounted ro at /etc/loom/network/ (NOT under /repo/.work)
  - Capability drop via gosu (container starts as root, exec'd as loom)
  - bypassPermissions ONLY when backend is Container (init AND spawn validation)
  - BackendType has ONE canonical definition with re-exports
- Architecture review (loom-code-reviewer):
  - TerminalBackend trait still cohesive
  - No new layering violations (daemon -> commands, etc.)
  - Send + Sync invariants preserved on ContainerBackend
  - Error handling via anyhow::Result with context throughout
  - No 400-line file violations
  - BackendDispatcher is the single routing point (no direct
       NativeBackend::new outside the factory)
  - .work/config.toml is the single source of truth for project backend
- Test coverage (loom-code-reviewer):
  - Permission resolution matrix (5 modes × 2 backends × stage types)
  - Runtime detection branches (with PATH mocking)
  - Fingerprint determinism (regression for findings #18, #19)
  - resolve_stage_backend matrix (narrowing only)
  - build_run_args snapshots
  - Container live tests gated on LOOM_TEST_CONTAINERS=1
1. Fix ALL issues found by reviewers using loom-software-engineer or
   loom-senior-software-engineer (loom-code-reviewer is read-only)

FUNCTIONAL VERIFICATION (MANDATORY):
8. CLI surface:

- ./target/debug/loom init --help shows --backend native|container and --no-build
- ./target/debug/loom container --help lists build / rebuild / doctor / shell
- ./target/debug/loom container doctor runs successfully (no panic)
1. Code surface checks:
   - BackendType has ONE definition (rg -F "enum BackendType" src/ should yield exactly one match)
   - ContainerBackend impls TerminalBackend
   - create_backend factory routes Container variant
   - BackendDispatcher::for_session and for_stage exist
   - resolve_stage_backend enforces narrowing-only rule (test passes)
   - LivenessService is wired into monitor (rg "kill\(.*0\)" src/orchestrator/monitor/
     should yield NO matches indicating direct kill -0 calls)
   - Hard-coded "acceptEdits" GONE from src/git/worktree/settings.rs and src/hooks/generator.rs
2. Phase 6 — bypassPermissions opt-in:
    - validate_config rejects BypassPermissions when backend is Native (init AND spawn)
    - validate_config permits BypassPermissions when backend is Container
    - apply_default_mode writes "bypassPermissions" to settings JSON when resolved

LIVE CONTAINER TESTS (gated on LOOM_TEST_CONTAINERS=1 AND container runtime available):
11. If `command -v podman` or `command -v docker` succeeds AND env var set:
    - loom stage complete from inside container with .work discovery via
      LOOM_WORK_DIR=/repo/.work works
    - Hook paths resolve and execute inside container (worktree-isolation.sh, commit-guard.sh)
    - Standard, merge, base-conflict, knowledge sessions use correct
      container paths (LOOM_WORKTREE_PATH=/repo/.worktrees/{stage} or /repo)
    - Monitor detects container death via <runtime> inspect (kill the
      container, monitor flags Crashed)
    - loom sessions kill terminates a Container session by name
    - loom clean --sessions removes orphan loom-*containers and loom-net-* networks
    - Native + bypassPermissions rejected at init AND spawn
    - Container + bypassPermissions writes "defaultMode": "bypassPermissions"
    - Sensitive env (SSH_AUTH_SOCK, GH_TOKEN, GITHUB_TOKEN) NOT in container env
      unless explicitly forwarded
    - Claude credentials policy: NO ~/.claude/.credentials.json mount when
      forward_credentials = []; mount present when ["claude"] (default)
    - Denied egress:
      *Public domain not in allowlist (curl fails)
      * IPv6 (curl -6 fails — ip6tables OUTPUT DROP)
      *DNS exfil to 8.8.8.8 (fails — only resolv.conf nameservers allowed)
      * 169.254.169.254 metadata (fails — explicit REJECT)
      *Sibling container (cross-stage curl fails — separate networks)
      * RFC1918 except loopback (fails — explicit REJECT)
    - Image cache invalidates when EITHER Dockerfile.tmpl OR firewall.sh
      changes (compute_fingerprint regression test)
    - Re-init: preserve backend (no flag), switch backend (--backend X),
      refuse loom run after --no-build
12. If no runtime available: document explicitly in integration-verify
    summary; record loom memory note "container live tests skipped: no
    podman/docker on host"; SKIP these tests but verify code surface
    remains valid

NOTE: Knowledge distillation and documentation updates are handled by
knowledge-distill. Focus this stage on BUILD + CODE REVIEW + FUNCTIONAL
VERIFICATION only. Record discoveries to loom memory so knowledge-distill
can curate them.

#### Files Changed

No changes recorded.

#### Key Decisions

- skip live container egress tests *(LOOM_TEST_CONTAINERS unset and sandbox has 'No network access allowed'. Signal explicitly authorises skipping with a memory note. Code-surface review and unit tests (docker_snapshot, podman_snapshot, fingerprint regression, resolve_stage_backend narrowing, validate_config matrix) provide the coverage for those paths.)*

#### Notes

- verified: LOOM_TEST_CONTAINERS env not set in this session and network disabled. Live container tests (egress denial matrix, container death detection, sessions kill of container) skipped per signal guidance; code-surface review covers spawn/network/firewall paths instead.
- verified: validate_config is wired at both init (commands/init/plan_setup.rs:13) and spawn (orchestrator/core/stage_executor.rs:255 and :378). BypassPermissions rejected on Native, accepted on Container in both paths (sandbox/config.rs:101).
- concern: src/orchestrator/terminal/container/mod.rs is 661 lines, over the 400-line CLAUDE.md guideline. Functional integration is correct; recommend splitting (e.g. extract spawn/mounts/env into submodules) in a follow-up refactor stage.
- found: forward_credentials default at init/execute.rs:238 is Vec::new() (empty), not the ["claude"] suggested in stage description. This is safer (explicit opt-in) — agents must edit .work/config.toml to mount ~/.claude/.credentials.json. Behavior is correct, default is stricter than spec.
- found: ContainerBackend env passing is allowlist-only via build_env_for_session (LOOM_*, CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS, GIT_ASKPASS). ENV_STRIP=[SSH_AUTH_SOCK, GH_TOKEN, GITHUB_TOKEN] is belt-and-suspenders — AWS_*/MCP_* never inherited because env is built explicitly. No leak vector unless allowlist is later widened.
- verified: build/test/clippy/fmt all green from worktree root + working_dir=loom. cargo test reports 1518 + 132 + 79 + 16 + 7 + 7 + 3 + 3 + 7 doc-tests, 0 failures. clippy --all-targets -D warnings clean.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Knowledge distillation — runs after integration-verify to curate all stage
memories into permanent knowledge files and update user-facing documentation.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):

1. Read plan file from doc/plans/IN_PROGRESS-PLAN-container-backend.md
2. Read codex-PLAN-container-backend.md (pressure-test) to ensure
   knowledge captures the lessons that drove the restructure
3. Read ALL stage memories: loom memory show --all
4. Read doc/loom/knowledge/*.md for current state
5. Read README.md and CLAUDE.md.template to identify sections needing updates

MEMORY CURATION:
6. Synthesize all stage memories
7. Curate mistakes as actionable prevention rules into doc/loom/knowledge/mistakes.md.
   High-priority entries (from pressure-test):

- Topology: container backends must preserve host repo structure inside
     the container or symlinks/git metadata break
- Persistence: plan-level config must be persisted to .work/ or it
     silently disappears on loom run reload
- Schema root: it's LoomConfig, not Plan
- Session identity: backend-aware operations need persisted backend metadata
- Liveness: monitor must route through TerminalBackend, not direct kill -0
- Run path coverage: every spawn site must use the dispatcher
     (foreground, daemon, merge_resolver, continuation, auto_merge)
- Init re-run: WorkDir initialization must support reconfigure
- Firewall mutability: agent-writable allowlist defeats the firewall
- Cache key completeness: hash every embedded resource that affects
     the image
1. Curate reusable patterns into doc/loom/knowledge/patterns.md:
   - TerminalBackend extension pattern (now proven with two impls)
   - BackendDispatcher / per-stage backend selection / per-session routing
   - Sandbox config merge with permission_mode resolution
   - Cross-platform runtime detection cfg pattern
   - Image cache + fingerprint pattern (global cache, per-project pin,
     complete cache key)
   - Firewall as defense-in-depth (image-resident script with capability
     drop + per-project allowlist mounted ro outside agent-writable mount)
   - Backend-aware liveness service pattern
   - Centralized config file ownership (toml_edit single API)
2. Update doc/loom/knowledge/architecture.md with the dual-backend topology,
   the new orchestrator/terminal/container/ + orchestrator/liveness.rs
   modules, and the run-path dispatcher integration
3. Update doc/loom/knowledge/conventions.md if any new conventions emerged
    (resources/ directory, toml_edit centralization, fingerprint format,
    kebab-case YAML for user-facing enums, embedded resources via include_str)
4. EXPLICITLY check existing knowledge files for any claim that BackendType
    is Native-only — these are now stale and must be updated
5. If 2+ stages hit the same mistake, document the root cause prominently

KNOWLEDGE REVIEW:
13. Run: loom review
14. Check for stale or outdated entries
15. Remove entries that no longer apply

DOCUMENTATION UPDATE (MANDATORY):
16. Update README.md to document:
    - --backend native|container flag on loom init
    - --no-build flag and the implication for loom run
    - loom container subcommands (build, rebuild, doctor, shell)
    - permission_mode field in plan/stage sandbox config (kebab-case YAML)
    - bypassPermissions opt-in (only with container backend)
    - forward_credentials model (default ["claude"], opt-in for github/gitlab/ssh-agent)
    - Threat model summary (deny IPv6, block 169.254.169.254, etc.)
17. Update CLAUDE.md.template if any new project-level conventions emerged
    (e.g., guidance on choosing --backend container in CI vs locally)
18. Only update sections relevant to the changes — do NOT rewrite entire files

FINAL CHECK:
19. loom memory list (verify all curated insights reflected in knowledge)
20. loom knowledge check (confirm coverage still healthy)

#### Files Changed

No changes recorded.

#### Key Decisions

- knowledge distillation: synthesize all 4 stage memories into architecture, patterns, mistakes, entry-points, and stack knowledge files; then update README.md with container backend features *(signal explicitly lists which high-priority entries to document from the pressure-test (codex))*

#### Notes

- concern: container/mod.rs is 661 lines, over the 400-line CLAUDE.md limit. Functional but needs follow-up refactor (extract spawn/mounts/env into submodules)
- found: forward_credentials default is Vec::new() (empty), not the spec's suggested ['claude']. This is safer — explicit opt-in. Users must edit .work/config.toml to enable credential forwarding.

## Open Questions

No open questions.
