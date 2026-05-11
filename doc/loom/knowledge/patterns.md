# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [conventions.md](conventions.md) for coding standards.

## Table of Contents

- [Architectural Patterns](#architectural-patterns)
  - [Table of Contents](#table-of-contents)
  - [State Machine Pattern](#state-machine-pattern)
  - [File-Based State Pattern](#file-based-state-pattern)
  - [Signal Generation Pattern](#signal-generation-pattern)
  - [Progressive Merge Pattern](#progressive-merge-pattern)
  - [Daemon IPC Pattern](#daemon-ipc-pattern)
  - [Polling Orchestration Pattern](#polling-orchestration-pattern)
  - [Monitoring Patterns](#monitoring-patterns)
  - [Hook Patterns](#hook-patterns)
  - [TUI Patterns](#tui-patterns)
  - [Knowledge Systems Pattern](#knowledge-systems-pattern)
  - [Stage Completion Pattern](#stage-completion-pattern)
  - [Goal-Backward Verification Pattern](#goal-backward-verification-pattern)
  - [Error Handling Pattern](#error-handling-pattern)
  - [Security Patterns](#security-patterns)
  - [Process Management Pattern](#process-management-pattern)
  - [Merge Anti-Respawn Pattern](#merge-anti-respawn-pattern)
  - [Permission Sync Pattern](#permission-sync-pattern)
  - [Sandbox Config Merging](#sandbox-config-merging)
  - [Directory Hierarchy Pattern](#directory-hierarchy-pattern)
  - [Three-Layer Guidance Reinforcement](#three-layer-guidance-reinforcement)
  - [Stage Necessity Test](#stage-necessity-test)
  - [Bootstrap Mode](#bootstrap-mode)
  - [Field Propagation Checklist](#field-propagation-checklist)
  - [Goal-Backward Verification Pattern \[UPDATED\]](#goal-backward-verification-pattern-updated)
  - [AcceptanceCriterion Design Pattern](#acceptancecriterion-design-pattern)
  - [Hook Content-Stripping Pattern](#hook-content-stripping-pattern)
  - [Hook Content-Stripping Pattern (Updated 2026-03-31)](#hook-content-stripping-pattern-updated-2026-03-31)
  - [Merge Recovery Flow \[UPDATED 2026-04-27\]](#merge-recovery-flow-updated-2026-04-27)
  - [Attribution-Aware Recovery (2026-04-27)](#attribution-aware-recovery-2026-04-27)
  - [Pure Routing Helper (2026-04-27)](#pure-routing-helper-2026-04-27)
  - [macOS GUI App Launch Pattern (2026-04-27)](#macos-gui-app-launch-pattern-2026-04-27)
  - [CLI Subcommand Registration Pattern](#cli-subcommand-registration-pattern)
  - [AcceptanceCriterion Untagged Enum](#acceptancecriterion-untagged-enum)
  - [Plan Validation Tier Separation (loom init contract)](#plan-validation-tier-separation-loom-init-contract)

---

## State Machine Pattern

Stage has 12 states: WaitingForDeps -> Queued -> Executing -> Completed (terminal). From Executing: Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview. Skipped is terminal. **Critical invariant**: dependents become Queued only when deps have `status == Completed AND merged == true`. Session has 6 states: Spawning -> Running -> Completed/Crashed/ContextExhausted, plus Paused<->Running. All transitions validated via `try_transition()`.

## File-Based State Pattern

All state persisted to `.work/` as markdown with YAML frontmatter. Benefits: git-friendly diffing, human-readable, crash recovery via file re-read. No explicit file locking; relies on daemon single-writer model. Stage files named with topological depth prefix (e.g., `01-knowledge-bootstrap.md`).

## Signal Generation Pattern

Uses Manus KV-cache optimization with four sections:

1. **Stable prefix** (~1000 bytes): Worktree rules, execution rules, CLAUDE.md reminders. SHA-256 hashed. Rarely changes. Includes self-review checklist (standard) or detailed review dimensions (integration-verify).
2. **Semi-stable** (~1500-2500 bytes): Knowledge refs, memory/knowledge management, agent teams, sandbox, skill recommendations. Changes per stage type.
3. **Dynamic** (variable): Target metadata, plan overview, dependency status, handoff content, git history, files, tasks. Changes per session.
4. **Recitation** (end): Memory entries (last 10), task state, critical context. Placed last for maximum attention weight.

Four stage-type-specific prefix generators: standard, knowledge, integration-verify, knowledge-distill. Six signal types: Regular, Knowledge, Recovery, Merge, MergeConflict, BaseConflict. Signals are self-contained via `EmbeddedContext` struct.

KnowledgeDistill prefix: focuses on memory reading and knowledge curation; includes `loom memory show --all` and `loom knowledge update` guidance; always uses sonnet.

**Data flow:** Stage Ready -> start_stage() -> create worktree -> Session.new() -> build_signal_context() -> format_signal_content() -> write_signal_file() -> spawn Claude Code.

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute: `Stage A completes -> Merge A to main -> Stage B starts`. Base branch resolution: no deps = init_base_branch or default; all deps merged = main; single dep not merged = dependency branch (legacy fallback). MergeLock prevents concurrent merges (30s timeout, 5min stale cleanup).

## Daemon IPC Pattern

Unix socket with 4-byte big-endian length-prefixed JSON (max 10MB). Supports SubscribeStatus (streaming 1s), Stop, Ping. Socket at `.work/orchestrator.sock`, mode 0o600, max 100 connections. Graceful shutdown: Stop -> shutdown_flag -> wait threads -> cleanup socket/PID. Drop ensures cleanup on panic.

## Polling Orchestration Pattern

Main loop polls every 5 seconds: sync graph from stage files, sync queued status, spawn merge resolution sessions, start ready stages, poll monitor for events, handle events. Exit when all stages complete or (failed + no sessions + no ready).

## Monitoring Patterns

**Heartbeat**: Sessions write to `.work/heartbeat/{stage-id}.json`. Timeout: 300s. PID alive + stale = Hung; PID dead = Crashed; PID dead + stage Completed = normal exit. **Context health**: Green (0-60%), Yellow (60-75% auto-summarize), Red (75%+ trigger handoff). Custom `context_budget` per stage (1-100%, default 65%). **Retry**: Exponential backoff `min(30 * 2^retry_count, 300s)`. Retryable: SessionCrash, Timeout. Non-retryable: ContextExhausted, TestFailure, BuildFailure, CodeError. Max 3 retries.

## Hook Patterns

Hooks receive data via **stdin JSON**. Read with `timeout 1 cat`. Response: exit 0 = allow, exit 2 = block (stderr shown). Advanced JSON response supports `permissionDecision: allow/deny/ask` with `updatedInput`.

**Key hooks**: commit-guard.sh (Stop) blocks exit without commit; commit-filter.sh (PreToolUse:Bash) blocks subagent commits; prefer-modern-tools.sh blocks grep/find; post-tool-use.sh updates heartbeat; pre-compact.sh triggers handoff; session-start/end.sh handle lifecycle.

**Subagent detection**: Wrapper script exports `LOOM_MAIN_AGENT_PID`. Hook compares `$PPID`. Subagents blocked from: git commit, git add -A/., loom stage complete.

Hook installation: scripts embedded via `include_str!()` in constants.rs, installed to `~/.claude/hooks/loom/`, config in `.claude/settings.local.json`.

## TUI Patterns

Two modes: **static** (one-time print) and **live** (real-time via daemon socket). Live uses ratatui with vertical layout: header(3), progress(3), main(min 10, two 50/50 columns), footer(3). Left: Executing(60%)+Pending(40%). Right: Completed(60%)+Blocked(40%). Three-layer cleanup: panic hook, Ctrl+C signal handler, Drop with `cleaned_up` flag.

## Knowledge Systems Pattern

Three systems: **Facts** (.work/facts.toml, cross-stage KV), **Memory** (.work/memory/{session}.md, session journal), **Knowledge** (doc/loom/knowledge/, permanent). Memory placed in signal recitation section for max LLM attention. Promotion: `loom memory promote`. Knowledge is append-only. Protected files marked with `<!-- .loom-protected -->`. Knowledge commands use `project_root()` (cwd-relative) so worktree agents write to their worktree, not the main repo.

## Stage Completion Pattern

**Regular stages**: Load stage, run acceptance criteria (unless --no-verify), sync worktree permissions, run task verifications, progressive merge, mark Completed, trigger dependents. **Knowledge stages**: No worktree, commits required (directly to main), auto merged=true, skips merge. Acceptance commands: 5-min timeout, support `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}` variables.

## Goal-Backward Verification Pattern

Three verification layers: **Truths** (shell commands, exit 0, extended: exit_code, stdout_contains, stderr_empty). **Artifacts** (files must exist, stub detection blocks TODO/FIXME/unimplemented!/todo!/pass/raise NotImplementedError). **Wiring** (grep patterns verify code connections). Required for `stage_type: standard` only. Limits: max 20 truths, 100 artifacts.

Before/after stage checks: before_stage runs AFTER worktree creation, BEFORE Executing (advisory). after_stage runs in complete.rs (blocking). Both use TruthCheck definitions.

Regression tests: `bug_fix: true` requires `regression_test` with file path and must_contain patterns. Bidirectional validation.

Advisory stderr warning detection: detect_stderr_warnings() in runner.rs scans for 9 suspicious patterns (connection refused, blocked, EACCES, etc.) after acceptance. Warnings only, no pass/fail change.

## Error Handling Pattern

`anyhow::Result<T>` throughout. Context via `.context()` and `.with_context()`. **Graceful degradation**: skill loading with warning fallback, `if let Ok()` for stage loading, `unwrap_or(false)` for liveness checks. Zero `unwrap()`/`expect()` in main code.

## Security Patterns

**Input validation**: `validate_id()` - alphanumeric + dash/underscore, max 128 chars, reserved names blocked. `safe_filename()` strips traversal. **Shell escaping**: `escape_shell_single_quote()` and `escape_applescript_string()` in emulator.rs. **Self-update**: minisign signature verification (50MB binary, 4KB sig), atomic install via temp->backup->rename->rollback. **Env var expansion**: positional replacement to handle overlapping names ($FOO vs $FOOBAR).

## Process Management Pattern

**Wrapper script** (pid_tracking.rs): Creates `.work/wrappers/{stage_id}-wrapper.sh`, sets env vars, writes PID, then `exec claude`. **PID discovery**: file read first, then `/proc` scan (Linux) or `ps aux`/`lsof` (macOS). **Liveness**: PID file -> kill -0 -> session.pid -> window by title. **Zombie prevention**: `spawn_reaper_thread()` calls `wait()`.

## Merge Anti-Respawn Pattern

When merge conflict session dies unresolved: session removed from `active_sessions`, signal file KEPT as anti-respawn guard. `spawn_merge_resolution_sessions()` checks `has_merge_signal_for_stage()` before spawning. Signal removed only when merge succeeds.

## Permission Sync Pattern

Three-component: path transformation (absolute->relative, parent traversal resolved), merge-not-overwrite (union+dedup), sync before acceptance. File locking via fs2 crate; always write to the locked handle.

## Sandbox Config Merging

Plan-level SandboxConfig merges with stage-level. Stage overrides plan. excluded_commands concatenate. Output: settings.local.json with sandbox.enabled, autoAllowBashIfSandboxed, network allowlist, permissions.

## Directory Hierarchy Pattern

Three-level: **Project Root**, **Worktree** (`.worktrees/<stage-id>/`), **working_dir** (YAML field). Path resolution: `EXECUTION_PATH = worktree_root + working_dir`. All acceptance/artifact/wiring paths relative to working_dir. Common mistake: `cargo test` failing because working_dir not set to Cargo.toml directory.

## Three-Layer Guidance Reinforcement

New agent guidance should be reinforced at: (1) Skill file (depth), (2) CLAUDE.md.template (authority), (3) cache.rs signals (runtime enforcement). Ensures guidance reaches agents regardless of entry point.

## Stage Necessity Test

Before creating stages: Q1: Does it create code another imports? Q2: Does it write files another writes? Q3: Does it need a verification checkpoint? If ALL NO -> merge into one stage with parallel subagents.

## Bootstrap Mode

`loom knowledge bootstrap` defaults to interactive mode (Stdio::inherit) for macOS compatibility. `--quick` opts into non-interactive (Stdio::null + -p flag). Exit codes 130/2 treated as user interrupt.

## Field Propagation Checklist

When adding new fields to StageDefinition: (1) plan/schema/types.rs, (2) models/stage/types.rs + Default, (3) commands/init/plan_setup.rs mapping, (4) plan/schema/tests/mod.rs make_stage(), (5) ALL test files constructing Stage, (6) validation.rs rules, (7) fs/stage_loading.rs, plan/graph/tests.rs, models/stage/methods.rs.

## Goal-Backward Verification Pattern [UPDATED]

Four verification layers: **Artifacts** (files must exist, stub detection blocks TODO/FIXME/unimplemented\!/todo\!/pass/raise NotImplementedError). **Wiring** (grep patterns verify code connections). **Wiring Tests** (runtime commands with success criteria). **Dead Code Check** (command + fail/ignore patterns).

Truths were removed as a standalone verification layer and unified into the acceptance field as AcceptanceCriterion::Extended(TruthCheck). Required for `stage_type: standard` and `integration-verify` — must have acceptance OR goal-backward checks.

Before/after stage checks: before_stage runs AFTER worktree creation, BEFORE Executing (advisory). after_stage runs in complete.rs (blocking). Both use TruthCheck definitions via verify_truth_checks() in truths.rs.

## AcceptanceCriterion Design Pattern

Uses `#[serde(untagged)]` enum with two variants:

- `Simple(String)` — plain shell command, deserializes from YAML string
- `Extended(TruthCheck)` — output validation, deserializes from YAML object with `command` field

Serde tries variants in order: strings match Simple first, objects fail Simple then match Extended. Error messages for malformed objects are poor (inherent untagged limitation). helper methods: `command()`, `is_extended()`, `Display` delegates to `command()`.

## Hook Content-Stripping Pattern

Hooks that validate bash commands must strip embedded text content before
pattern matching. The strip_embedded_content() function (in hooks/\_common.sh
for shell, validators/bash.rs for Rust) removes:

1. Heredoc bodies (awk state machine tracking <<MARKER to MARKER)
2. -m / --message quoted content (sed replacements)

Each hook sources \_common.sh via: source "$(dirname "$0")/\_common.sh"

Full hook inventory (13 scripts in hooks/):

- PreToolUse: worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
  prefer-modern-tools.sh, worktree-file-guard.sh
- PostToolUse: post-tool-use.sh, ask-user-post.sh
- Stop: commit-guard.sh, learning-validator.sh
- SessionStart: session-start.sh
- SessionEnd: session-end.sh
- PreCompact: pre-compact.sh
- UserPromptSubmit: skill-trigger.sh, ask-user-pre.sh

## Hook Content-Stripping Pattern (Updated 2026-03-31)

All PreToolUse hooks that match command patterns MUST use `strip_embedded_content()` before pattern matching to prevent false positives from keywords appearing inside commit messages or heredoc bodies.

**Architecture:**

- `_common.sh` provides `strip_embedded_content()` (shared across all shell hooks)
- `loom/src/hooks/validators/bash.rs` provides Rust equivalent `strip_embedded_content()`
- Phase 1: awk state machine strips heredoc bodies (`<<MARKER` to `^MARKER$`)
- Phase 2: sed strips `-m`/`--message` quoted content

**Usage pattern:**

1. Source `_common.sh` at top of hook
2. Call `stripped=$(strip_embedded_content "$cmd")`
3. Use `$stripped` for pattern detection (git -C, .worktrees/, ../../, grep, find)
4. Use original `$cmd` for patterns that MUST match message body (e.g., Co-Authored-By)

**Commit-filter dual-check:**

- STRIPPED_COMMAND for detecting `git commit` (prevents "commit" in messages from triggering)
- ORIGINAL COMMAND for Co-Authored-By check (anchor `^` prevents mid-line false positives)

**Security posture:** All stripping failures result in false positives (overly strict), never bypasses (permissive). This is the correct safety direction for development hooks.

**Hooks using this pattern:** worktree-isolation.sh, commit-filter.sh, git-add-guard.sh, prefer-modern-tools.sh

## Merge Recovery Flow [UPDATED 2026-04-27]

MergeConflict -> bail\!() forces original session to exit -> commit-guard.sh allows exit for MergeConflict status -> detection.rs recognizes as normal exit -> spawn_merge_resolution_sessions() kills any stale original session, then spawns resolver -> merge signal includes "Inherited Responsibilities" section explaining resolver owns the stage -> user directed to `loom stage merge <stage-id> --resolved`.

Key invariant: the original execution session MUST exit when merge conflict is detected. Three mechanisms enforce this:

1. `bail\!()` in `complete_with_merge()` propagates error and terminates the session
2. `commit-guard.sh` does NOT block exit for MergeConflict status
3. `spawn_merge_resolution_sessions()` actively kills stale sessions before spawning resolver

**Daemon ordering invariant (2026-04-27):** Reconciliation runs BEFORE `sync_graph_with_stage_files` AND BEFORE `recover_orphaned_sessions`. Recovery deletes orphaned merge session files; attribution depends on their metadata. Sync reads stage files into the graph; if reconcile flips disk state AFTER sync, the graph keeps the stale view and would queue dependents based on a phantom merge.

**Daemon-off CLI parity (2026-04-27):** `loom stage complete` on a `Completed + merged=true` stage with an active main-repo merge attributed to it triggers the same revert the daemon performs (`Completed → MergeConflict + merged=false + merge_conflict=true`) before spawning the resolver. The router's `RevertAndSpawnResolver` arm encodes this; persistence is the caller's responsibility, BEFORE spawn so `spawn_merge_resolver`'s status contract is satisfied.

## Attribution-Aware Recovery (2026-04-27)

`MERGE_HEAD` in the main repo is global state — only one merge in progress at a time across all stages. Stage-state mutation triggered by detecting it must come with proof of attribution; without proof, refuse rather than mutate.

Three attribution sources (first match wins):

1. **MergeSession metadata** — orphaned or live `SessionType::Merge` with matching `merge_source_branch`.
2. **Branch HEAD match** — a `MERGE_HEAD` SHA equals `loom/<stage-id>` HEAD.
3. **Completed-commit match** — a `MERGE_HEAD` SHA equals `stage.completed_commit`.

**BaseConflict carve-out:** When current HEAD is `loom/_base/*` (or any session has `SessionType::BaseConflict` matching it), return `GlobalUnattributed` even if the merge heads contain a stage branch's commit. Multi-dependency base merges check out their own branch and run a merge there; their MERGE_HEAD must NOT mutate stage state.

Single decision point: `attribute_main_repo_merge` in `orchestrator/merge_attribution.rs`. Both daemon recovery (`reconcile_main_repo_active_merge`) and the CLI router consume it.

## Pure Routing Helper (2026-04-27)

`route_complete_for_conflicts` is the canonical example: read-only function that returns `CompleteConflictRoute` without writing to disk. Persistence is the caller's responsibility on the success path only. This preserves the "refusal preserves stage file state" invariant — refusal always leaves the stage file untouched, which is critical for tests and for users investigating why a completion attempt was rejected.

Apply this pattern when adding routing/verification helpers: keep the function pure, return an enum of decisions, let the caller persist on the success branch.

## macOS GUI App Launch Pattern (2026-04-27)

macOS apps installed in `/Applications/X.app` may ship a CLI binary inside `Contents/MacOS/` that is NOT added to PATH. To launch with arguments without requiring a manual PATH shim, use `open -na <AppName> --args <flags...>` from `Command::new("open")`. The CLI flags following `--args` are passed through to the new process exactly as if invoked directly — Ghostty's `--working-directory=`, `--title=`, and `-e CMD` all work this way (per Ghostty maintainer in ghostty-org/ghostty#9221).

**`-na` vs `-a`:** Always use `-na` (force new instance) when each invocation needs its own per-window args. With `-a`, an already-running singleton may ignore `--args` and just focus the existing window — `--working-directory` and `-e` would silently no-op. Trade-off: process accumulation, acceptable when each window corresponds to a finite stage.

**Where applied:** `emulator.rs` `Self::Ghostty` arm uses this on macOS while keeping the direct `ghostty <args>` invocation on Linux via `#[cfg(not(target_os = "macos"))]`. The arm-level cfg-gating pattern (rather than per-emulator-variant duplication) keeps cross-platform terminals together. Same approach applies to any future `.app`-distributed terminal emulator added to loom.

**When NOT to use:** Mac-only emulators (`TerminalApp`, `ITerm2`) already use AppleScript via `osascript`, which is itself PATH-independent — no `open` needed. Use `open -na ... --args` only when the underlying tool accepts CLI flags directly.

## CLI Subcommand Registration Pattern

Adding any new top-level command (e.g. `loom plan`) requires touching exactly **three files**:

1. **`loom/src/cli/types.rs`** — Add variant to `Commands` enum (with `#[command(subcommand)]` if nested):

   ```rust
   /// Validate a plan without side effects
   Plan {
       #[command(subcommand)]
       command: PlanCommands,
   },
   ```

2. **`loom/src/cli/dispatch.rs`** — Add match arm in `dispatch()`:

   ```rust
   Commands::Plan { command } => match command {
       PlanCommands::Verify { path, strict } => plan::verify(path, strict),
   },
   ```

   Also add the module import at the top: `use loom::commands::plan;`

3. **`loom/src/commands/newcmd.rs`** (or `commands/newcmd/mod.rs`) — Implement the execute function.
   Then expose it from `loom/src/commands/mod.rs`: `pub mod newcmd;`

**Verification**: `cargo build` must pass. `loom <newcmd> --help` must show the command.

**Nested subcommands**: define a second `#[derive(Subcommand)]` enum in `cli/types.rs` (e.g. `PlanCommands`), mirror the outer pattern. See `types_stage.rs` / `types_memory.rs` for examples of extracted sub-enum files.

## AcceptanceCriterion Untagged Enum

`AcceptanceCriterion` in `plan/schema/types.rs` is a `#[serde(untagged)]` enum:

```rust
#[serde(untagged)]
pub enum AcceptanceCriterion {
    Simple(String),        // YAML: - "cargo test"
    Extended(TruthCheck),  // YAML: - command: "cargo test"\n  exit_code: 0
}
```

**Serialization**: serde tries each variant in declaration order. A plain YAML string deserializes to `Simple`; a mapping with a `command` key deserializes to `Extended(TruthCheck)`.

**Accessing the command**: use `.command()` method — works for both variants.

**`TruthCheck`** fields: `command`, optional `exit_code` (default 0), optional `stdout_contains`, optional `stderr_empty`.

**Why untagged**: avoids requiring a `type: simple` / `type: extended` discriminator in user-authored YAML. The trade-off is that serde error messages on malformed input are less precise.

## TerminalBackend Extension Pattern

Adding a new backend requires:

1. Implement `TerminalBackend` trait (spawn_session, spawn_merge_session, spawn_base_conflict_session, spawn_knowledge_session, kill_session, is_session_alive)
2. Add a variant to `BackendType` enum in `plan/schema/execution.rs`
3. Add construction logic to `orchestrator/terminal/mod.rs::create_backend()`
4. Update `BackendNeeds` + `BackendDispatcher::for_plan()` in `dispatcher.rs`
5. Update `sandbox/config.rs::validate_config()` for any permission restrictions
6. Wire liveness through `LivenessService` (not direct `kill -0`)

Two proven implementations: `NativeBackend` (host terminal, 11+ emulators) and `ContainerBackend` (Docker/Podman/Apple Container).

## BackendDispatcher Pattern

`BackendDispatcher` is the single source-of-truth for which backends are constructed. Callers declare `BackendNeeds` (which backends a plan uses) up-front; the dispatcher only constructs what's needed. Routing uses the session's persisted `backend` field (written at spawn time) — survives daemon restarts.

```rust
// Read from stage files to determine needs, then:
let dispatcher = BackendDispatcher::for_plan(project_backend, needs, work_dir)?;
let liveness = LivenessService::new(Arc::new(dispatcher));
// All spawn/kill/alive calls go through dispatcher
```

## Backend-Aware Liveness Pattern

Never `kill -0` directly for container sessions — the PID is inside the container namespace. Use `LivenessService::is_alive(session)` which routes via the session's `backend` metadata to the appropriate implementation (host `kill -0` for native, `<runtime> inspect` for containers).

For tests: `LivenessService::fixed_for_tests(bool)` — returns a fixed value without constructing a backend. Avoids spinning up Docker/process table for monitor unit tests.

## Image Cache + Fingerprint Pattern

Global image cache: `~/.local/share/loom/images/<fingerprint>.json`. Per-project digest pin in `.work/config.toml`. Cache key (fingerprint) encodes:

1. Sorted list of detected language canonical names
2. SHA-256 of embedded `Dockerfile.tmpl` content
3. SHA-256 of embedded `firewall.sh` content

Any change to languages detected or either embedded resource produces a distinct fingerprint → automatic rebuild. Fingerprint format: `"<langs>-<hex[:8]>"` (e.g. `"rust-typescript-a3b9ef12"`). `compute_fingerprint_inner` is testable (takes content as args; `compute_fingerprint` calls it with `include_str!` constants).

## Firewall as Defense-in-Depth (Container)

Firewall script lives inside the image (not on the host filesystem) — agents cannot edit it. Mounted allowlist is host-owned and mounted ro at `/etc/loom/network/allowed_domains.txt`. This separation means:

- Agent writes to `/etc/loom/...` are refused by the container
- Only `loom` (host process) can update the allowlist
- Firewall denies: IPv6 (`AF_INET6`), `169.254.169.254` (cloud metadata), `127.0.0.0/8` except `127.0.0.1`, `*.internal`

Write allowlist before container start: `network::write_allowlist(work_dir, &network_config)`.

## Container Topology Invariant

Host repo root MUST be bind-mounted at a fixed container path (`/repo`) with git worktrees preserved. The relative symlink `.work -> ../../.work` inside each worktree only resolves correctly when the full repo tree (including `.worktrees/`) is present at the mount point. Do NOT mount just the stage worktree — git and loom metadata break.

Stage cwd in container: `/repo/.worktrees/<stage-id>`. Merge/knowledge cwd: `/repo`. `LOOM_WORK_DIR=/repo/.work` set explicitly.

## Sandbox permission_mode Resolution

`permission_mode` resolves: stage-level > plan-level > stage-type default.

| Stage type | Default permission_mode |
| --- | --- |
| Standard / IntegrationVerify | `auto` |
| Knowledge / KnowledgeDistill | `accept-edits` |

`bypass-permissions` ONLY allowed when `BackendType::Container`. `validate_config(merged, backend)` in `sandbox/config.rs` enforces this — called at both init and spawn time.

YAML key is `permission_mode` (snake_case), values are kebab-case: `"auto"`, `"accept-edits"`, `"bypass-permissions"`, `"plan"`, `"default"`.

## Centralized Config File Ownership (toml_edit)

All writes to `.work/config.toml` go through `fs/work_dir.rs` using `toml_edit` for round-trip-safe writes. `toml` is for typed reads. Never mix: `toml_edit Item -> serde` silently drops nested sub-tables.

`read_section::<T>` re-parses the whole file with `toml::Value` then `try_into` on the section — preserves nested config sub-tables.

## Cross-Platform Runtime Detection (cfg pattern)

Runtime-specific behavior (e.g., Apple Container vs Docker on macOS) can be gated at source level with `#[cfg(target_os = "macos")]` per code path. The `runtime.rs::is_apple_container` check requires both `/usr/local/bin/container` to exist AND `container --version` to return Apple-signature output — `which::which("container")` alone would collide with unrelated tools.

## Plan Validation Tier Separation (loom init contract)

`loom init` runs validation in two distinct tiers that `loom plan verify` must mirror:

**Tier 1 — Fatal (blocks init):**

- `plan/schema/validation.rs::validate(&metadata)` — called inside `parse_and_validate()` inside `parse_plan_content()`
- Returns `Err(Vec<ValidationError>)` on failure; parse aborts, init fails immediately
- Checks: unsupported version, duplicate stage IDs, unknown deps, path traversal, empty acceptance, artifact path safety, wiring regex validity, bug_fix/regression_test consistency

**Tier 2 — Advisory (printed, never block):**

- `validate_structural_preflight(&stages, repo_root)` — warnings for double-path prefixes, weak wiring patterns, missing build config files, before/after check imbalance
- `check_knowledge_recommendations(&stages)` — warns if plan has no knowledge-bootstrap stage
- `check_sandbox_recommendations(&metadata)` — warns if `loom` not in `excluded_commands`, or `allow_unsandboxed_escape` is true
- All return `Vec<String>`; init prints them and continues

**`loom plan verify` contract:** run `parse_plan()` first (auto-runs Tier 1); if it returns `Err`, report fatal errors and exit non-zero. If it succeeds, run the three Tier 2 functions, print their warnings, exit 0 (advisory only).

**Call site:** `loom/src/commands/init/plan_setup.rs` — shows the canonical order and how warnings are surfaced to the user.

## Container CLI Defense-in-Depth: Query Runtime Before Trusting Session File

Container CLI commands (`loom container logs`, `loom container shell`, `loom container list`) resolve the container name from `.work/sessions/*.md`. Session files are NOT deleted on container removal, so a populated `container_name` field is not proof of container liveness.

**Pattern:** Before exec-ing into `<runtime> logs|exec|inspect`, call:

```bash
<runtime> inspect -f '{{.State.Status}}' <container_name>
```

Non-zero exit or "no such container" → container is gone. Fall back to `.work/crashes/<stage>-*.container.log` for post-mortem log access. This makes the CLI command robust to stale session files from completed, crashed, or cleaned stages.

**Implementation:** `list.rs::query_container_status()` is the canonical helper — it returns `"running"`, `"exited"`, `"missing"`, or `"error: ..."`. Reuse it in `logs.rs` / `shell.rs` rather than reimplementing.

**Why:** Defense-in-depth. The session file is a cache; the runtime is the authoritative source. Cross-checking prevents confusing errors ("no such container") from reaching the user.

---

## Lifecycle Documentation Belongs in the Module That Owns the Lifecycle

Lifecycle rules for per-stage containers (when created, when removed, what happens on crash) belong in the `//\!` module doc of `orchestrator/terminal/container/mod.rs`, not scattered across callers. This is the single module that owns container lifetime — all other modules call into it.

**Pattern:** When a module owns the full lifecycle of a resource (create → run → cleanup), document that lifecycle contract in the module's top-level `//\!` comment block. Callers reference this doc rather than re-explaining the rules.

**Applied to:** `orchestrator/terminal/container/mod.rs` — documents when containers are removed (spawn failure, kill_session, loom stop, loom clean) and the log-capture invariant (best-effort, never blocks removal).

---

## Session Identity Symmetry: Setter + Clearer Must Travel Together

Every field group on `Session` that represents a runtime resource identity (`container_name` + `runtime` for containers, `pid` for processes) requires a matching setter AND clearer method.

| Field group | Setter | Clearer | Called after |
|---|---|---|---|
| `runtime` + `container_name` | `set_container_identity()` | `clear_container_identity()` | Container removed |
| `pid` | `set_pid()` | *(no clearer — PID is fixed for session lifetime)* | N/A |

**Rule:** Any caller that removes the runtime resource (container rm, process kill) must call the clearer before persisting the session file. `clear_container_identity()` is in `models/session/methods.rs:135`.

**Why:** Without the clearer, session files become permanent references to removed containers. Lookups in `loom container logs` / `loom container list` will find stale entries and produce confusing errors.
