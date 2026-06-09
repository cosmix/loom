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
  - [Session Identity: Setter + Clearer Must Travel Together](#session-identity-setter--clearer-must-travel-together)

---

## State Machine Pattern

Stage has 12 states: WaitingForDeps -> Queued -> Executing -> Completed (terminal). From Executing: Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview. Skipped is terminal. **Critical invariant**: dependents become Queued only when deps have `status == Completed AND merged == true`. Session has 6 states: Spawning -> Running -> Completed/Crashed/ContextExhausted, plus Paused<->Running. All transitions validated via `try_transition()`.

## File-Based State Pattern

All state persisted to `.work/` as markdown with YAML frontmatter. Benefits: git-friendly diffing, human-readable, crash recovery via file re-read. Stage files named with topological depth prefix (e.g., `01-knowledge-bootstrap.md`).

**Concurrency is NOT single-writer.** Three writer classes mutate stage files concurrently: the orchestrator main loop (`orchestrator/core/persistence.rs::save_stage`), the daemon dispute IPC thread (`daemon/server/dispute.rs`), and agent-run CLI commands (`commands/stage/{complete,merge,check_acceptance,skip_retry}.rs`). All stage-file reads/writes go through `fs/locking.rs` advisory `flock`s on the **parent directory** inode (`stages/`), and writes are crash-atomic (temp-file + `rename`). See the Locked Stage Read-Modify-Write Pattern below — the old "no explicit file locking; single-writer model" assumption was false and produced the A-5 lost-update class.

## Locked Stage Read-Modify-Write Pattern (A-5)

`locked_read`/`locked_write` serialize *individual* reads/writes, but the load → mutate → save flow releases the lock between load and save. Each `save_stage` serializes the **entire** `Stage`, so a writer that loaded the stage minutes earlier (e.g. `loom stage complete` holding a stage across a multi-minute acceptance run) reverts any field a concurrent writer changed in the gap — a lost update (status reverted, `dispute_count`/`retry_count`/`close_reason`/`session`/amended `acceptance` clobbered).

**Fix — `verify::transitions::update_stage(stage_id, work_dir, |s| { ... })`:** holds the `stages/` directory lock across a *fresh* on-disk read, the closure, and the crash-atomic write. The closure mutates the **current** persisted `Stage`, so it only touches the fields the operation owns; a concurrent writer's other fields survive. Returns the written `Stage`. The file must already exist (creation still uses `save_stage`). A closure `Err` leaves the file untouched.

```rust
// Re-read under the lock, apply only the operation-owned delta:
update_stage(stage_id, work_dir, |s| {
    s.dispute_count = s.dispute_count.saturating_add(1); // incremented from on-disk value
    s.try_request_adjudication(reason)                   // status transition validated on-disk
})?;
```

Underlying primitives (`fs/locking.rs`): `locked_dir_update(dir, f)` locks a directory inode for the duration of `f` (for find-read-write when the file's exact prefixed path is unknown); `atomic_write_locked(path, content)` is the temp+rename write used *inside* a held lock.

**Field-ownership rule (the judgment-heavy part):** for a LONG operation, re-apply ONLY the fields that operation owns and leave every other field at its on-disk value. Ownership as migrated: progressive-merge completion owns `completed_commit`/`merged`/`merge_conflict`/status (`progressive_complete.rs`); `loom stage merge` owns `fix_attempts` + the merge-completion transition (`merge.rs`); `check_acceptance` owns only `fix_attempts`; the dispute handler owns `dispute_count`/`evidence_rounds`/status (`dispute.rs`); the adjudicator verdict owns status/`review_reason`/`evidence_rounds`/`amendments_applied`/`acceptance`/`wiring` (`adjudication/mod.rs`); plan amendment owns only `acceptance`/`wiring` (`plan/amendment.rs`, per the Adjudicator Scope Convention).

**Long-op shape:** run the slow work (git merge under its own `MergeLock`, acceptance commands) OUTSIDE the stages-dir lock, then apply the owned fields in a SHORT `update_stage` closure — never hold the stages-dir lock across git/subprocess work.

**Invariants preserved:** never write `merged=true` without ancestry verification (the `merged=true` writes in `merge.rs`/`merge.rs --resolved` follow a real merge or a `verify_or_derive_completed_commit` ancestry check, both done before the closure); `route_complete_for_conflicts` stays a pure read-only seam (no early whole-`Stage` save before its decision); status transitions still go through `try_*`/`force_status_with_reason`.

**Not migrated (deliberate):** orchestrator main-loop `save_stage` sites in `recovery.rs`/`merge_handler.rs`/`event_handler.rs`/`stage_executor.rs`. They operate on a stage freshly read into the graph in the same tick (`sync_graph_with_stage_files` re-reads disk every tick), the loop is single-threaded, and they do not overlap the dispute/adjudication field set. Migrating all ~40 would be a large blast radius across the merge/recovery lifecycle for no realized-lost-update benefit.

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

## Session Spawning Pattern

`NativeBackend` (`orchestrator/terminal/native/`) is the single concrete type
for spawning Claude Code sessions in host terminal windows. It exposes
`spawn_session`, `spawn_merge_session`, `spawn_base_conflict_session`,
`spawn_knowledge_session`, `kill_session`, and `is_session_alive`. The
orchestrator holds it as `Arc<NativeBackend>` and shares it with the
`LivenessService`; every spawn site (main loop, foreground spawner,
merge_handler, continuation, auto_merge) uses the same `Arc<NativeBackend>`.

```rust
let native = Arc::new(NativeBackend::new(work_dir)?);
let liveness = LivenessService::new(Arc::clone(&native));
// All spawn/kill/alive calls go through `native`.
```

## Liveness Pattern

Use `LivenessService::is_alive(session)` rather than calling `kill -0` directly.
This routes through `NativeBackend::is_session_alive`, keyed on the session's
`tracking_key` so prefixed merge/knowledge/base-conflict sessions resolve
correctly.

For tests: `LivenessService::fixed_for_tests(bool)` — returns a fixed value without constructing a backend.

## Sandbox permission_mode Resolution

`permission_mode` resolves: stage-level > plan-level > stage-type default.

| Stage type | Default permission_mode |
| --- | --- |
| Standard | `accept-edits` |
| IntegrationVerify | `accept-edits` |
| Knowledge | `accept-edits` |
| KnowledgeDistill | `accept-edits` |

All four stage types default to `accept-edits` as of 2026-05-14. Override at plan or stage level with `permission_mode: auto` if needed.

YAML key is `permission_mode` (snake_case), values are kebab-case: `"auto"`, `"accept-edits"`, `"plan"`, `"default"`.

## Centralized Config File Ownership (toml_edit)

All writes to `.work/config.toml` go through `fs/work_dir.rs` using `toml_edit` for round-trip-safe writes. `toml` is for typed reads. Never mix: `toml_edit Item -> serde` silently drops nested sub-tables.

`read_section::<T>` re-parses the whole file with `toml::Value` then `try_into` on the section — preserves nested config sub-tables.

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

**Known gap (2026-05-14):** `loom plan verify` does NOT validate `sandbox.permission_mode=bypass-permissions`. That check lives only in `sandbox::config::validate_config`, called from `commands/init/plan_setup.rs` (init path) and at spawn time. `plan verify` skips it, so a plan with `bypass-permissions` reports 0 errors from `plan verify` but fails at `loom init`. Fix: thread `validate_config` into the `plan verify` flow.

**Call site:** `loom/src/commands/init/plan_setup.rs` — shows the canonical order and how warnings are surfaced to the user.

## Session Identity: Setter + Clearer Must Travel Together

Every field group on `Session` that represents a runtime resource identity requires a matching setter AND clearer method.

| Field group | Setter | Called after |
|---|---|---|
| `pid` | `set_pid()` | Session spawned |

**Rule:** Any caller that releases a runtime resource must call the matching clearer before persisting the session file.

## reqwest::blocking HTTP Client Pattern

Template from `commands/self_update/client.rs` — mirror this for the adjudicator:

```rust
use reqwest::blocking::Client;

fn create_http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))  // includes all transfer time
        .user_agent("loom-adjudicator")     // change per consumer
        .build()
        .context("Failed to create HTTP client")
}

fn validate_response_status(response: &reqwest::blocking::Response, context: &str) -> Result<()> {
    if !response.status().is_success() {
        bail!("HTTP {} {}: {}", response.status().as_u16(),
              response.status().canonical_reason().unwrap_or("Unknown"), context);
    }
    Ok(())
}
```

`reqwest::blocking::Client` is already a dependency (used by self_update); no new Cargo.toml entry needed for the adjudicator.

## Worker Thread + mpsc Pattern (New — Adjudicator)

The adjudicator introduces loom's first worker-thread + mpsc pattern. Template:

```rust
// In Orchestrator struct:
worker_completion_tx: mpsc::Sender<WorkerCompletion>,
worker_completion_rx: mpsc::Receiver<WorkerCompletion>,

// On NeedsAdjudication transition:
let tx = self.worker_completion_tx.clone();
std::thread::spawn(move || {
    let verdict = call_anthropic_api(&dispute_request);
    let _ = tx.send(WorkerCompletion { stage_id, verdict });
});

// In main loop tick (drain channel):
while let Ok(completion) = self.worker_completion_rx.try_recv() {
    self.apply_adjudicator_verdict(completion)?;
}
```

Worker crashes leave no verdict file; staleness detection: `.inflight` marker with timestamp, >10min → re-fire (bounded by `adjudicator_attempt_count` cap of 3).

## Dispute File Authority Split Pattern

Three-file trust boundary to prevent self-approval attacks:

| File | Writer | Content | Rationale |
|------|--------|---------|-----------|
| `request.md` | Daemon (on agent's behalf via RPC) | Agent's evidence payload | Agent can read but never write directly |
| `verdict.md` | Daemon worker thread only | Verdict + citations | Stage agents never write here — daemon-authored only |
| `applied.marker` | Daemon only (zero-byte) | Idempotency guard | Prevents re-application on restart |

If the agent could write both request and verdict, it could pre-fill `verdict: Accept` and self-approve. The split enforces the trust boundary at the filesystem level.

## Plan Amendment Atomic Write Pattern

For amending the IN_PROGRESS plan file safely (Stage 3):

```text
1. Acquire .work/plan_versions/.lock  (file lock — serializes concurrent amendments)
2. Compute new plan content in memory
3. Atomic-write .work/plan_versions/<n>.md  (full snapshot)
4. Append to .work/plan_versions/audit.md  (O_APPEND — atomic for small rows)
5. Atomic temp+rename of IN_PROGRESS plan file to new content
6. Release lock
```

Recovery on crash: scan audit.md for latest amendment; verify plan file matches snapshot. If mismatch → restore from `<n>.md`. If `<n>.md` missing → discard audit row, use `<n-1>.md`.

Note: `plan/graph/loader.rs:60-86` PREFERS `.work/stages/` files over the plan file. Plan-file amendment MUST also update the corresponding `.work/stages/<stage_id>.md` for the change to be reflected in the running orchestrator graph.

## NeedsHumanReview Orchestrator Handling Pattern

For new `NeedsAdjudication` state, mirror the existing `NeedsHumanReview` pattern:

1. `orchestrator/monitor/detection.rs:87-92` — emit `MonitorEvent::StageNeedsHumanReview` on transition detection
2. `orchestrator/core/event_handler.rs:142-158` — print banner + notify
3. `orchestrator/core/recovery.rs:814` — `StageStatus::NeedsHumanReview => continue` (skip auto-retry)
4. `orchestrator/core/recovery.rs:515-526` — sync status to in-memory graph

Add parallel handling for `NeedsAdjudication` that fires the worker thread instead of continuing.

## Remote Control Capability/Preflight/Resolve Pattern (2026-05-14)

`--remote-control` requires claude >= 2.1.51 AND claude.ai login auth (no disqualifying env var, `~/.claude/.credentials.json` present). Because the flag exits non-zero on failure, it must never be passed unconditionally.

**Three-function split:**

| Function | What it does | When to call |
|----------|-------------|--------------|
| `preflight(path)` | Runs `claude --version` + auth eligibility check | Startup advisory only |
| `resolve(work_dir)` | Per-spawn gate (mode + marker + memoized preflight) | Called at every spawn site |
| `write_unsupported_marker(work_dir)` | Writes `.work/remote_control-unsupported` | Called by crash_handler on fast-fail |

**`resolve()` check order (all cheap):**

1. `[remote_control] mode = off` in `.work/config.toml` → false (operator opted out)
2. `.work/remote_control-unsupported` marker exists → false (mid-run fast-fail)
3. Memoized `preflight()` via `OnceLock` (runs `claude --version` at most once per process) → true/false

**Fast-fail fallback (crash_handler.rs):**

- Session crashes within 15 seconds of creation while `resolve()` is true → write unsupported marker → retry with `--remote-control` omitted.
- No new retry code path: the existing exponential-backoff retry handles it; `resolve()` returning false is the only change.

**`build_claude_command()` helper (native/mod.rs):**

Pure function shared by all four spawn sites (`spawn_session`, `spawn_merge_session`, `spawn_base_conflict_session`, `spawn_knowledge_session`). Signature:

```rust
fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    remote_control_enabled: bool,
    escaped_prompt: &str,
) -> String
```

Appends `--remote-control` before the prompt positional only when `remote_control_enabled` is true. Call `resolve(work_dir)` to compute the flag, then pass the bool into this helper.

**OnceLock memoization note:**

`cached_preflight_enabled()` uses a process-lifetime `OnceLock<bool>`. This is intentional: `claude --version` output is invariant for the lifetime of a daemon process. Config (`mode`) and the marker file are re-read on every `resolve()` call (both cheap) so operator changes or crash-handler writes take effect immediately without restarting the daemon.
