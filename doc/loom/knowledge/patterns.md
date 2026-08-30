# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [conventions.md](conventions.md) for coding standards.

## Table of Contents

Superseded by the generated index. Read the files directly, or run
`loom knowledge context --query "..."` for a targeted pull; open
[INDEX.md](INDEX.md) for the current tier-1 / tier-2 map — a hand-maintained
table of contents goes stale the moment a topic is added.

## State Machine Pattern

Stage has 13 states: WaitingForDeps -> Queued -> Executing -> Completed (terminal). From Executing: Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview, and NeedsAdjudication. Skipped is terminal. **Critical invariant**: dependents become Queued only when deps have `status == Completed AND merged == true`. Session has 6 states: Spawning -> Running -> Completed/Crashed/ContextExhausted, plus Paused<->Running. All transitions validated via `try_transition()`.

## File-Based State Pattern

All state persisted to `.work/` as markdown with YAML frontmatter. Benefits: git-friendly diffing, human-readable, crash recovery via file re-read. Stage files named with topological depth prefix (e.g., `01-knowledge-bootstrap.md`).

**Concurrency is NOT single-writer.** The orchestrator loop, daemon IPC handlers, and agent-run CLI commands all mutate stage files. Existing-record changes must use the canonical locked `update_stage` transaction; crash-atomic replacement alone does not prevent stale logical writes. See the Locked Stage Read-Modify-Write Pattern below.

## Locked Stage Read-Modify-Write Pattern (A-5)

`locked_read`/`locked_write` serialize _individual_ reads/writes, but the load → mutate → save flow releases the lock between load and save. Each `save_stage` serializes the **entire** `Stage`, so a writer that loaded the stage minutes earlier (e.g. `loom stage complete` holding a stage across a multi-minute acceptance run) reverts any field a concurrent writer changed in the gap — a lost update (status reverted, `dispute_count`/`retry_count`/`close_reason`/`session`/amended `acceptance` clobbered).

**Fix — `verify::transitions::update_stage(stage_id, work_dir, |s| { ... })`:** holds the `stages/` directory lock across a _fresh_ on-disk read, the closure, and the crash-atomic write. The closure mutates the **current** persisted `Stage`, so it only touches the fields the operation owns; a concurrent writer's other fields survive. Returns the written `Stage`. The file must already exist; creation uses `create_stage` (`save_stage` is a creation-only compatibility alias and refuses overwrites). A closure `Err` leaves the file untouched.

```rust
// Re-read under the lock, apply only the operation-owned delta:
update_stage(stage_id, work_dir, |s| {
    s.dispute_count = s.dispute_count.saturating_add(1); // incremented from on-disk value
    s.try_request_adjudication(reason)                   // status transition validated on-disk
})?;
```

Underlying primitives (`fs/locking.rs`): `locked_dir_update(dir, f)` locks a directory inode for the duration of `f` (for find-read-write when the file's exact prefixed path is unknown); `atomic_write_locked(path, content)` is the temp+rename write used _inside_ a held lock.

**Field-ownership rule (the judgment-heavy part):** for a long operation, re-apply only the fields that operation owns and leave every other field at its freshly read on-disk value. Progressive merge owns completed commit/merge/status fields; merge retry owns `fix_attempts` and its merge transition; dispute/adjudication owns its review counters, status, and amendment fields; plan amendment owns only the amendable verification policy.

**Long-op shape:** run the slow work (git merge under its own `MergeLock`, acceptance commands) OUTSIDE the stages-dir lock, then apply the owned fields in a SHORT `update_stage` closure — never hold the stages-dir lock across git/subprocess work.

**Invariants preserved:** never write `merged=true` without ancestry verification (the `merged=true` writes in `merge.rs`/`merge.rs --resolved` follow a real merge or a `verify_or_derive_completed_commit` ancestry check, both done before the closure); `route_complete_for_conflicts` stays a pure read-only seam (no early whole-`Stage` save before its decision); status transitions still go through `try_*`/`force_status_with_reason`.

**No orchestrator exemption:** although the scheduler loop is single-threaded, daemon IPC and CLI writers are concurrent. Recovery, merge, crash, completion, and event handlers therefore apply the same short locked deltas as commands. Whole-record persistence is reserved for actual stage creation.

## Signal Generation Pattern

Uses Manus KV-cache optimization with four sections:

1. **Stable prefix** (~1000 bytes): Worktree rules, execution rules, CLAUDE.md reminders. SHA-256 hashed. Rarely changes. Includes self-review checklist (standard) or detailed review dimensions (integration-verify).
2. **Semi-stable** (~1500-2500 bytes): Knowledge refs, memory/knowledge management, agent teams, sandbox, skill recommendations. Changes per stage type.
3. **Dynamic** (variable): Target metadata, plan overview, dependency status, handoff content, git history, files, tasks. Changes per session.
4. **Recitation** (end): Memory entries (last 10), task state, critical context. Placed last for maximum attention weight.

Four stage-type-specific prefix generators: standard, knowledge, integration-verify, knowledge-distill. Six signal types: Regular, Knowledge, Recovery, Merge, MergeConflict, BaseConflict. Signals are self-contained via `EmbeddedContext` struct.

KnowledgeDistill prefix: focuses on memory reading and knowledge curation; includes `loom memory show --all` and `loom knowledge update` guidance. The stage itself runs on **opus** (every `StageType` defaults to opus); it is the _spot-read subagents_ the prefix tells the main agent to delegate to that are sonnet.

**Data flow:** Stage Ready -> start_stage() -> create worktree -> Session.new() -> build_signal_context() -> format_signal_content() -> write_signal_file() -> spawn Claude Code.

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute: `Stage A completes -> Merge A to main -> Stage B starts`. Base branch resolution: no deps = init_base_branch or default; all deps merged = main; single dep not merged = dependency branch (legacy fallback). MergeLock prevents concurrent merges (30s timeout, 5min stale cleanup).

## Daemon IPC Pattern

Unix socket at `.work/orchestrator.sock`, created mode 0o600 under a mode-0700 `.work/` directory.
Each request starts with a fixed authentication preface, so invalid credentials are rejected before
allocating the JSON body. Requests are capped at 64 KiB, responses at 2 MiB, and reads use an
absolute five-second deadline. Admission is bounded by 8 workers, a 16-request queue, a 512 KiB
global in-flight request budget, and 32 subscribers per stream. User capabilities cover Ping,
status/log subscriptions, Unsubscribe, DisputeCriteria, and the data-only `CompleteStage` request.
Completion is accepted only for the exact active stage/session identity and remains under the
sessions-directory lock through the stage transition; replay and cross-stage/session requests fail
without mutating state. Stop requires a one-time action-bound operator proof. A stable-file `flock`
is authoritative for daemon ownership and is held for the server lifetime. Graceful shutdown sets
the shutdown flag, joins bounded workers, and removes only the control files owned by that daemon.

## Polling Orchestration Pattern

Main loop polls every 5 seconds: sync graph from stage files, sync queued status, spawn merge resolution sessions, start ready stages, poll monitor for events, handle events. Exit when all stages complete or (failed + no sessions + no ready).

## Monitoring Patterns

**Heartbeat**: Sessions write to `.work/heartbeat/{stage-id}.{session-id}.json` (session-keyed, see below). Timeout: 300s. PID alive + stale = Hung; PID dead = Crashed; PID dead + stage Completed = normal exit. The heartbeat carries real resident tokens (`context_tokens`, `transcript_path`) written by `hooks/post-tool-use.sh` from the transcript tail — `context_percent` no longer exists. **Context health**: `orchestrator/monitor/context.rs::context_health(tokens, ceiling)` bands the ratio Green `<60%`, Yellow `60-90%`, Red `>=90%` of the resolved `context_ceiling_tokens` (absolute tokens, default 150,000; per-stage override in tokens, not a percentage) — there is no auto-summarize step. **Retry**: Exponential backoff `min(30 * 2^retry_count, 300s)`. Retryable: SessionCrash, Timeout. Non-retryable: ContextExhausted, TestFailure, BuildFailure, CodeError. Max 3 retries.

## Hook Patterns

Hooks receive data via **stdin JSON**. Read with `timeout 1 cat`. Response: exit 0 = allow, exit 2 = block (stderr shown). Advanced JSON response supports `permissionDecision: allow/deny/ask` with `updatedInput`.

**Key hooks**: commit-guard.sh (Stop) blocks exit without commit; commit-filter.sh (PreToolUse:Bash) blocks subagent commits; subagent-verify-guard.sh (PreToolUse:Bash) blocks subagent full-suite verification; plans-path-guard.sh (PreToolUse:Edit/Write) blocks plan writes outside `doc/plans/`; prefer-modern-tools.sh blocks grep/find; post-tool-use.sh updates heartbeat; pre-compact.sh triggers handoff; session-start/end.sh handle lifecycle.

**Subagent detection**: Wrapper script exports `LOOM_MAIN_AGENT_PID`. `loom_is_subagent()` requires that PID to be a live ancestor, then classifies the caller payload-first via `loom_payload_agent_verdict` (`.agent_type`/`.transcript_path`); an intervening-Claude-process walk is only the fallback for a payload-less or unrecognized caller — it is not a `$PPID` comparison. Subagents are blocked from git mutation and stage completion.

Hook installation: scripts embedded via `include_str!()` in constants.rs, installed to `~/.claude/hooks/loom/`, config in `.claude/settings.local.json`.

## TUI Patterns

Two modes: **static** (one-time print) and **live** (real-time via daemon socket). Live uses ratatui with vertical layout: header(3), progress(3), main(min 10, two 50/50 columns), footer(3). Left: Executing(60%)+Pending(40%). Right: Completed(60%)+Blocked(40%). Three-layer cleanup: panic hook, Ctrl+C signal handler, Drop with `cleaned_up` flag.

## Knowledge Systems Pattern

Three systems, in ascending order of permanence:

| System            | Location                                                                  | Lifetime                          | Written by                                                                   |
| ----------------- | ------------------------------------------------------------------------- | --------------------------------- | ---------------------------------------------------------------------------- |
| **Memory**        | `.work/memory/{session}.md`                                               | The run                           | `loom memory note\|decision\|change\|question`                               |
| **Stage outputs** | `outputs: Vec<StageOutput>` on the stage file (key / value / description) | The run; read by dependent stages | `loom stage output set`                                                      |
| **Knowledge**     | `doc/loom/knowledge/` (tiered)                                            | Permanent                         | `loom knowledge update` (stage execution) or direct Write/Edit (interactive) |

Memory is placed in the signal's recitation section for maximum LLM attention. The promotion path from memory to knowledge is the **`knowledge-distill` stage**, which reads `loom memory show --all` and curates — there is no `loom memory promote` command.

`loom knowledge update` appends; `loom knowledge replace-section <file> <heading> [content]` overwrites a `## <heading>` section's body in place — the correction path for stale knowledge — and falls back to appending, with a distinct message, when the heading is not found. There is still no verb that deletes a section outright, or renames its heading (see concerns.md). Knowledge commands resolve through `WorkDir::project_root()` (cwd-relative), so a worktree agent writes to its own worktree rather than the main repo.

**Corrected 2026-07-30:** an earlier version of this section claimed a `.work/facts.toml` cross-stage KV store, a `loom memory promote` command, and `<!-- .loom-protected -->` file markers. None of the three exist in the codebase. Cross-stage KV is `loom stage output`; "Discovered Facts" survives only as a HandoffV2 field and a signal sub-section.

## Stage Completion Pattern

**Regular stages**: Load stage, run acceptance criteria (unless --no-verify), sync worktree permissions, run task verifications, progressive merge, mark Completed, trigger dependents. **Knowledge stages**: No worktree, commits required (directly to main), auto merged=true, skips merge. Acceptance commands: 5-min timeout, support `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}` variables.

## Error Handling Pattern

Application and orchestration boundaries use `anyhow::Result<T>` with `.context()` or
`.with_context()`. Domain operations use typed errors when a caller must distinguish outcomes, such
as `BaseBranchError`, `MergeProbeError`, and `ProcessTimeoutError`; adapters such as Clap validators
may return strings because their interface requires display text. Do not stringify a domain error
before a caller has finished matching it, and do not add a second general-purpose error framework.
Graceful degradation is explicit and limited to operations whose callers do not require recovery
semantics, such as optional skill discovery or best-effort notification.

## Security Patterns

**Input validation**: `validate_id()` - alphanumeric + dash/underscore, max 128 chars, reserved names blocked. `safe_filename()` strips traversal. **Shell escaping**: `escape_shell_single_quote()` and `escape_applescript_string()` in emulator.rs. **Self-update**: minisign signature verification (50MB binary, 4KB sig), atomic install via temp->backup->rename->rollback. **Env var expansion**: positional replacement to handle overlapping names ($FOO vs $FOOBAR).

## Process Management Pattern

**Wrapper script** (`pid_tracking.rs`): creates `.work/wrappers/{stage_id}-wrapper.sh`, starts from `env -i`, reconstructs a minimal locale/terminal allowlist plus explicit Loom variables, records PID and process start time, then `exec`s the agent. **Liveness/signaling** uses `process::ProcessIdentity`; start-time mismatch is dead and missing identity is unverifiable. Raw PID fallback is forbidden. **Zombie prevention:** `spawn_reaper_thread()` calls `wait()`.

## Merge Anti-Respawn Pattern

When merge conflict session dies unresolved: session removed from `active_sessions`, signal file KEPT as anti-respawn guard. `spawn_merge_resolution_sessions()` checks `has_merge_signal_for_stage()` before spawning. Signal removed only when merge succeeds.

## Permission Sync Pattern

Three-component: path transformation (absolute->relative, parent traversal resolved), merge-not-overwrite (union+dedup), sync before acceptance. File locking via fs2 crate; always write to the locked handle.

## Sandbox Config Merging

Plan-level `SandboxConfig` merges with stage-level policy, with stage values overriding plan values. Plan-configured `excluded_commands` are rejected outright; sandbox disablement and unsandboxed escape require explicit policy acknowledgement or are rejected. Generated settings emit OS-level `denyRead` for sensitive paths and set `failIfUnavailable: true` whenever the sandbox is enabled. A settings-write failure blocks the stage before spawn. Loom, Git, interpreters, build tools, and package managers are never granted prefix-wide unsandboxed Bash access.

## Directory Hierarchy Pattern

Three-level: **Project Root**, **Worktree** (`.worktrees/<stage-id>/`), **working_dir** (YAML field). Path resolution: `EXECUTION_PATH = worktree_root + working_dir`. All acceptance/artifact/wiring paths relative to working_dir. Common mistake: `cargo test` failing because working_dir not set to Cargo.toml directory.

## Three-Layer Guidance Reinforcement

New agent guidance should be reinforced at: (1) Skill file (depth), (2) CLAUDE.md.template (authority), (3) cache.rs signals (runtime enforcement). Ensures guidance reaches agents regardless of entry point. Agent definitions (`agents/*.md`) serve as a supplementary fourth surface for role-specific guidance (e.g., coordinator/worker roles in subagent hierarchies).

### Mini Adversarial Code Review (multi-surface, 2026-06-25)

Every code-producing stage must end with a MANDATORY mini adversarial code review across six fixed dimensions: **code quality & architecture (SOLID), idiomatic code, security, wiring, dead & unnecessary code, no duplication (DRY across the whole codebase)**. The doctrine is reinforced across six surfaces that MUST stay consistent when the dimensions change:

| Surface                    | Where                                                                                                                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Runtime signal (canonical) | `orchestrator/signals/cache.rs::append_adversarial_review()` — injected into Standard + IntegrationVerify stable prefixes (via `stable_prefix_for`, so the recovery path gets it too) |
| Authority                  | `CLAUDE.md.template` — Stage Completion Checklist "MINI ADVERSARIAL CODE REVIEW" block                                                                                                |
| Implementer agents         | `agents/loom-software-engineer.md`, `agents/loom-senior-software-engineer.md` — "Self-Review Before Returning"                                                                        |
| Reviewer agent             | `agents/loom-code-reviewer.md` — Capabilities aligned to the six dimensions                                                                                                           |
| Plan authoring             | `skills/loom-plan-writer/SKILL.md` — note under stage_type table (auto-injected; don't restate in descriptions)                                                                       |

Scope rule: code stages ONLY. Documentation stages (`knowledge`, `knowledge-distill`) emit only markdown and deliberately omit it; cache + recovery tests negative-assert its absence there. Silent-failure detection is a SEPARATE concern (Standard has its own block; IV has `SILENT FAILURE DETECTION`) — not part of the six dimensions.

## Stage Necessity Test

Before creating ANY stage beyond the bookends, it must answer YES to one of four questions
(`skills/loom-plan-writer/SKILL.md:388`) and the plan prose must NAME which one:

- **Q1** — does another stage need this stage's code _merged_ before it can start? Only a
  MERGE-ORDER dependency counts. "B imports A" is compile-order → foundation step in ONE stage.
- **Q2** — does another stage write files this stage also writes? (file conflict)
- **Q3** — does later work need a verification checkpoint on this first? Name what would go
  undetected without it; "it would be tidy" is not a checkpoint.
- **Q4** — would the combined work blow a single session's context budget?

All NO → merge into ONE stage with parallel subagents over disjoint files. See
[Stage Fragmentation](mistakes.md) for the detection rule and the cost of getting this wrong.

## Field Propagation Checklist

When adding new fields to StageDefinition: (1) plan/schema/types.rs, (2) models/stage/types.rs + Default, (3) commands/init/plan_setup.rs mapping, (4) plan/schema/tests/mod.rs make_stage(), (5) ALL test files constructing Stage, (6) validation.rs rules, (7) fs/stage_loading.rs, plan/graph/tests.rs, models/stage/methods.rs.

## Goal-Backward Verification Pattern

Four verification layers: **Artifacts** (files must exist, stub detection blocks TODO/FIXME/unimplemented\!/todo\!/pass/raise NotImplementedError). **Wiring** (grep patterns verify code connections). **Wiring Tests** (runtime commands with success criteria). **Dead Code Check** (command + fail/ignore patterns).

**Truths is NOT a layer here.** It was removed as a standalone goal-backward layer and unified into the acceptance field as `AcceptanceCriterion::Extended(TruthCheck)`. A duplicate section claiming "Three verification layers: Truths, Artifacts, Wiring" was deleted from this file on 2026-07-30. Required for `stage_type: standard` and `integration-verify` — must have acceptance OR goal-backward checks.

Validation limits (`plan/schema/validation.rs`): max 100 artifacts; max 20 `before_stage` checks; max 20 `after_stage` checks.

Before/after stage checks: before_stage runs AFTER worktree creation, BEFORE Executing — blocking (failed check → stage Blocked, no session spawned), and only while the workspace is pristine (skipped once the stage branch/worktree holds prior work; see architecture.md). after_stage runs in complete.rs (blocking). Both use TruthCheck definitions via `verify_truth_checks()` in truths.rs.

Regression tests: `bug_fix: true` requires `regression_test` with file path and must_contain patterns. Bidirectional validation.

Advisory stderr warning detection: `detect_stderr_warnings()` in runner.rs scans for 9 suspicious patterns (connection refused, blocked, EACCES, etc.) after acceptance. Warnings only, no pass/fail change.

## AcceptanceCriterion Design Pattern

Uses `#[serde(untagged)]` enum with two variants:

- `Simple(String)` — plain shell command, deserializes from YAML string
- `Extended(TruthCheck)` — output validation, deserializes from YAML object with `command` field

Serde tries variants in order: strings match Simple first, objects fail Simple then match Extended. Error messages for malformed objects are poor (inherent untagged limitation). helper methods: `command()`, `is_extended()`, `Display` delegates to `command()`.

## Hook Command Matching

Hooks decide what a Bash command INVOKES by scanning argv tokens, not by regexing the command
string — a regex cannot tell an argument's _value_ from its _mention_, so quoted prose was read as
shell. Stripping heredoc and `-m` bodies is now the pre-step; the old regexes survive only as the
unterminated-quote fallback. Path checks key on whitespace, since a real path argument is a
whitespace-free word and a prose payload is not.

→ [Hook Content-Stripping](patterns/hook-content-stripping.md)

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

### ⚠️ Gotcha: Clap is only HALF the registration — dynamic completions are a separate site

The three files above make a command **compile, dispatch, and show in `--help`**, but loom ships a **second, hand-maintained completion engine** that does NOT read Clap's metadata. A command registered only via the three-file pattern is **invisible to shell tab-completion** (and its flags won't complete). The completion tables are hardcoded string lists in `loom/src/completions/dynamic/`:

- `commands.rs` — `TOP_LEVEL_COMMANDS` (the top-level name list; keep it alphabetical), `complete_flags` (per-command-path flag arms, e.g. `["pressure"] => &["--rounds", "--dry-run"]`), `complete_subcommands` + `has_subcommands` (only for commands with nested sub-enums).
- `mod.rs` — `complete_after_command` routes the positional arg of a single-level command. A command whose positional is a **file path** (like `init`) returns `Ok(Vec::new())` so the shell falls back to native path completion; a command taking a stage ID calls `complete_stage_ids`.
- `tests/tests_commands.rs` — add a test asserting the new command appears in top-level completion and that its flags complete.

**Rule:** "register a CLI command" in this repo = Clap (3 files) **AND** the dynamic-completion tables + their tests. Before assuming Clap is the whole story, `rg TOP_LEVEL_COMMANDS loom/src/completions`. This is easy to miss because the command works end-to-end in manual testing and `--help` — only tab-completion silently lacks it.

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

## Session Spawning and Liveness Pattern

The orchestrator holds one `Arc<SessionBackend>` and shares it with `LivenessService`. Spawn resolves
the native or tmux lane per call and records the chosen lane on `Session.backend`; kill and liveness
dispatch by that persisted lane. Use `LivenessService::is_alive(session)` rather than raw PID probes.
Process identity is PID plus kernel start time, and destructive signaling fails closed when identity
cannot be verified. For tests, `LivenessService::fixed_for_tests(bool)` avoids constructing a backend.

## Sandbox permission_mode Resolution

`permission_mode` resolves: stage-level > plan-level > stage-type default.

| Stage type        | Default permission_mode |
| ----------------- | ----------------------- |
| Standard          | `auto`                  |
| IntegrationVerify | `auto`                  |
| Knowledge         | `auto`                  |
| KnowledgeDistill  | `auto`                  |

All four stage types default to `auto` as of 2026-07-01 (previously `accept-edits`). Loom stages execute autonomously with no human at the terminal, so the agent auto-accepts actions its heuristics deem safe; the sandbox filesystem deny/allow rules and hooks are the safety boundary. Override at plan or stage level with a stricter `permission_mode` (e.g. `accept-edits`, `plan`) if needed.

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
- `check_sandbox_recommendations(&metadata)` — rejects command-prefix exclusions and flags other unsafe sandbox expansion such as `allow_unsandboxed_escape`
- All return `Vec<String>`; init prints them and continues

**`loom plan verify` contract:** run `parse_plan()` first (auto-runs Tier 1); if it returns `Err`, report fatal errors and exit non-zero. If it succeeds, run the three Tier 2 functions, print their warnings, exit 0 (advisory only).

**Known gap (2026-05-14):** `loom plan verify` does NOT validate `sandbox.permission_mode=bypass-permissions`. That check lives only in `sandbox::config::validate_config`, called from `commands/init/plan_setup.rs` (init path) and at spawn time. `plan verify` skips it, so a plan with `bypass-permissions` reports 0 errors from `plan verify` but fails at `loom init`. Fix: thread `validate_config` into the `plan verify` flow.

**Call site:** `loom/src/commands/init/plan_setup.rs` — shows the canonical order and how warnings are surfaced to the user.

## Session Identity: Setter + Clearer Must Travel Together

Every field group on `Session` that represents a runtime resource identity requires a matching setter AND clearer method.

| Field group | Setter      | Called after    |
| ----------- | ----------- | --------------- |
| `pid`       | `set_pid()` | Session spawned |

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

| File             | Writer                             | Content                  | Rationale                                            |
| ---------------- | ---------------------------------- | ------------------------ | ---------------------------------------------------- |
| `request.md`     | Daemon (on agent's behalf via RPC) | Agent's evidence payload | Agent can read but never write directly              |
| `verdict.md`     | Daemon worker thread only          | Verdict + citations      | Stage agents never write here — daemon-authored only |
| `applied.marker` | Daemon only (zero-byte)            | Idempotency guard        | Prevents re-application on restart                   |

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

The three-phase shape for driving an external agent binary: detect capability, preflight the
environment, then resolve the concrete invocation. Keeps unsupported combinations failing
early with an actionable message instead of mid-run.

→ [Remote Control Pattern](patterns/remote-control.md)

## Subagent Hierarchy + Ultracode Guidance (2026-06-12)

When to fan out flat, when to use a 2-level coordinator→worker hierarchy, and when to reach
for agent teams; the model mix for each; and the file-exclusivity rule that makes parallel
subagents safe. Also carries the no-verify doctrine subagents inherit. Ultracode Workflow
fan-out is Claude-only — the codex lane runs outside it via normal `loom-codex-forwarder`
spawns — and a plan should prefer one ultracode stage over several parallel sibling stages
doing the same operation on disjoint file sets.

→ [Subagent Hierarchy](patterns/subagent-hierarchy.md)

## Synchronous Foreground Agent Driver (`loom pressure`)

A second execution model distinct from the daemon/worktree orchestrator: `loom pressure` (commands/pressure/mod.rs) spawns external agents synchronously in the foreground. The reusable sub-patterns:

- **Foreground spawn, inherited stdio:** children run via `Command::status()` (blocking) with `Stdio::inherit()` for stdin/stdout/stderr, in `current_dir(repo_root)`. No terminal backend, no session tracking, no `.work/`. Use this shape when a command orchestrates interactive tools the user must watch live, rather than background stages.
- **Single-source argv builders:** `claude_args()`/`codex_args()` are the ONLY place argv is assembled, consumed by BOTH the real spawn (`spawn_*`) and `render_dry_run`. `--dry-run` can therefore never drift from what actually runs. Apply whenever a command has a preview/plan mode.
- **Sibling-report naming + pre-delete guard:** the Codex review is written to `codex-<basename>` next to the plan. The report is deleted at the START of every round so that if Codex fails to write a fresh review, the following `/address` cannot silently read the previous round's stale report; a final delete cleans up after the last round.
- **Repo-relative invocation, not cwd-relative:** `resolve_plan_path` derives the agent argument via `fs_path.strip_prefix(repo_root)` (repo-relative when under the repo, else absolute) because children run with `current_dir(repo_root)`, not the user's shell cwd. It gates on `is_file()` (not `exists()`, which is true for dirs) and falls back to `doc/plans/<arg>` only when the raw path is absent AND does not already start with `doc/plans/` (double-prefix guard).
- **Visible exit classification:** `classify_code` maps `0`→continue, `130`/`2`/`None`→abort, other→warn — but the abort/warn handlers PRINT the child label + exit code, so a headless failure (e.g. a codex clap usage error exiting 2) is surfaced rather than mistaken for a clean Ctrl+C interrupt.

## Doctrine Blocks, Fail-Safe Gates, and Shell Matchers (2026-07-28)

Guidance that must appear byte-identically on several surfaces needs an equality test, not N
greps; privilege lookups from state files must treat ambiguity as refusal; and shell-command
classification in a hook has a specific normalise-then-tokenise shape.

→ [Doctrine & Fail-Safe Patterns](patterns/doctrine-cross-surface.md)

## Tiered Knowledge Hierarchy (2026-07-28)

The knowledge base is **two-tier**. Tier-1 files (`architecture.md`, `patterns.md`, …) hold
short summaries that link out; tier-2 topics live at `<category>/<slug>.md` and hold the detail.
`INDEX.md` is generated and is the single layout predicate — a directory is hierarchical **iff**
`INDEX.md` exists.

Reading protocol: index first, then the tier-1 summary for your area, then only the tier-2
topics you actually touch. Writing protocol: a tier-1 section that grows past ~40 lines is spilled
into a topic and replaced by a 2-4 line summary plus a relative link.

The link form `[Title](category/slug.md)` in a **tier-1** file is the house convention —
relative, `.md` extension, no `./`, no anchor. No audit enforces it today; an earlier version of
this doc claimed two link-form checks required exactly this form, but no such checks exist in the
tree. See [Knowledge Hierarchy](architecture/knowledge-hierarchy.md) for the mechanics and the
`catalog::build` diagnostics that actually run.

## Model Playbook: Orchestration Is Always Opus (2026-07-28)

Every `StageType` now defaults to **opus** (`models/stage/types.rs::default_model`), with
`default_reasoning_effort()` returning `xhigh` whenever the effective model is opus. Judgement-
heavy orchestration work — planning, distillation, review, verification — is never downgraded to
save tokens.

Savings come from **delegation, not downgrade**: an opus main agent spawns implementation
subagents by agent type across four tiers — fable (visual/UI design, a bug that survived a
delegated fix attempt, extremely challenging algorithmic design; no agent type pins it, so the
model override is explicit at spawn), opus (`loom-senior-software-engineer`, mainstream architecture and algorithm
implementation), sonnet or GPT-5.6 Terra (`loom-software-engineer` or the `loom-codex-forwarder`
codex lane, common implementation and integration tests), and GPT-5.6 Luna (codex lane,
boilerplate, scaffolding, simple unit tests). The codex tiers are licensed only on stages listing
codex in `implementers`; elsewhere terra- and luna-tier work goes to sonnet. The codex tiers also
require the `codex` CLI and plugin to be installed; when either is missing, `loom run` warns at
startup (never aborts) and the same terra-/luna-to-sonnet fallback applies for the run — see
[Codex Plugin](architecture/codex-plugin.md). A knowledge stage runs on opus and delegates its
code spot-reads to sonnet; the two facts are easy to conflate.

## Session Backend Dispatch Details

The orchestrator holds
`Arc<SessionBackend>` (`orchestrator/core/orchestrator.rs:91`, constructed at `:148` via
`SessionBackend::from_config`), and shares that same `Arc` with the `LivenessService`
(`orchestrator/liveness.rs:17,32`):

```rust
let backend = Arc::new(SessionBackend::from_config(config.work_dir.clone())?);
let liveness = LivenessService::new(Arc::clone(&backend));
```

`SessionBackend` dispatches each call to the `Native` or `Tmux` lane. Two rules follow:

- **Spawn** resolves the lane per call (config + tmux availability + fallback marker), then records the
  lane actually used on `Session.backend`.
- **Kill and liveness** dispatch on `session.backend` — the lane that _spawned_ it — never on the
  currently-configured backend, so sessions survive a config change or a daemon restart.

Every spawn site uses the shared handle; the other `SessionBackend::from_config` callers are
`orchestrator/continuation/mod.rs:89`, `commands/sessions.rs:130`, `commands/stage/state.rs:57`,
`commands/stage/merge_resolver.rs:72` and `commands/stage/skip_retry.rs:254`.

`LivenessService::fixed_for_tests(bool)` still returns a fixed value without constructing a backend.

→ [Terminal Backends](architecture/terminal-backends.md)

## Extract the Decision When the Failure Mode Is Not Reproducible in CI (2026-08-08)

**Problem shape:** a rule guards an OS failure that no CI runner can produce. The test written against
the real command passes _for the wrong reason_ and would still pass with the rule deleted.

Concretely: the e2e case meant to pin tmux's "exit 0 but stderr non-empty is a failure" rule used an
unwritable socket parent — which makes tmux exit **1**, so the plain exit-code check alone satisfied
it. The genuine condition needs the socket dir to exist while socket _creation_ is denied.

**Pattern:** split the rule into a pure decision fn over already-gathered inputs
(`evaluate_new_session(socket, status_success, stderr)`) and unit-test _that_. The impure caller keeps
only the gathering. Applied again in `build_overview_argv`, which takes the viewer socket and
`(socket, tracking_key)` pairs as parameters rather than deriving them, so the whole argv sequence is
assertable without tmux.

**Rule:** when an OS failure mode is not reproducible in CI, extract the decision and test it directly.
Never settle for a test that passes for the wrong reason — see `mistakes/tests-that-cannot-fail.md`.

## Fail-Safe Direction for Destructive Sweeps (2026-08-08)

Any sweep that kills or deletes must resolve _uncertainty_ toward inaction:

- **Cannot read the evidence ⇒ do not destroy.** `tmux/socket.rs`'s `socket_session_is_alive` returns
  `false` for an absent session file but **`true`** for one that exists and cannot be parsed — a file
  caught mid-write must not be read as "dead".
- **Cannot positively attribute ⇒ do not destroy.** Reap only resources provably owned by _this_ work
  dir. Shared per-user namespaces (the tmux socket dir) make "no matching state file" match other
  checkouts' live resources.
- **Report what you skipped.** Unattributable resources are surfaced to the user, never silently
  killed and never silently ignored.

## One Flattener for an Untrusted Value Rendered on Many Surfaces

`context/untrusted.rs::inline_safe` is the single definition both agent-facing renderers
call — the Knowledge Brief (`signals/format/brief.rs`) and `loom knowledge context`'s stdout
(`commands/knowledge/context.rs`). Its docstring states outright that a second copy would
duplicate a security rule that must never drift.

The generalisable shape: when the same escaping or fencing rule must hold on N surfaces,
put it in one function that all N import, and say so in the docstring — because the failure
mode is not a wrong implementation, it is a second implementation. And remember the surface
set is larger than it looks: a brief whose footer advertises a command extends the boundary
to that command's output. See `mistakes/untrusted-value-boundaries.md`.

## Confidence Ceilings as Named Constants

When a component's view is structurally narrower than the claim it is asked to make, encode
the gap as a numeric ceiling in a named constant whose docstring carries the reasoning —
not as a comment at the call site. The source graph does this with
`MAX_INFERRED_CONFIDENCE = 0.5` (an extractor sees one file), `MAX_RESOLVED_INFERRED_CONFIDENCE = 0.9`
(whole-graph uniqueness is evidence, but not a parse) and `1.0` reserved for `Parser`
provenance alone.

Two properties make it work: a widening is a **new constructor** encoding the wider bound
(`SourceEdge::resolve_to`), never a raw field write; and path-level aggregation takes the
**MINIMUM** edge confidence along a path, never a product — a product punishes long
fully-parsed chains for their length (`1.0 × 1.0 × 1.0` stays `1.0`, but `0.9^5` reads as a
guess). `resolve.rs`'s `Trust::extend` only lowers the running minimum, carrying the weakest
edge's provenance and kind with it.

## Base Layer Plus Per-Stage Overlay, Shadowing Wholesale

The pattern that lets parallel worktrees share derived state safely: an immutable base keyed
by the revision it was built from, plus a per-stage overlay holding only what that stage
changed, read as `overlay ∪ (base − overlay's files)`. An overlay entry shadows its base
counterpart **wholesale, never merges with it** — a partial merge produces a view describing
no revision that ever existed.

Reusable rules that come with it: the layout module owns layering and serialization only,
never building or write-timing; and the layering cannot express a DELETION without a
tombstone concept, so plan for one if deletions matter.
→ [architecture/context-retrieval.md](architecture/context-retrieval.md)

## Best-Effort By Contract, Stated In the Docstring

`telemetry` and `context::delivery` are both declared optimisations that may never fail the
operation they observe: `emit` discards its own error, `read_events` skips a malformed line
rather than failing the file, a missing delivery directory reads as "nothing delivered".
Writing that contract into the module docstring is what stops a later author "fixing" the
swallowed error into a propagated one.

The corollary is the failure budget rule: **the durable result and the derived artifact have
different budgets — never let the cheaper one veto the expensive one.** A reconcile failure
marks derived state stale and leaves a good merge intact.

## One Door for an Irreversible Operation

`MergeLifecycle::cleanup` is the only path by which any caller may reach
`cleanup_after_merge`, and it refuses unless the stage branch is provably contained in the
target. Removing a side effect from a function is only durable if that side effect gains a
single owner — otherwise the next caller re-adds it locally. Pair it with the rule that a
destructive step which cannot verify its own precondition must decline rather than proceed.
→ [mistakes/merge-cleanup-boundary.md](mistakes/merge-cleanup-boundary.md)

## Matched Positive and Negative Controls for a Boundary Test

`verify/criteria/tests/confine_tests.rs` ships
`confined_shell_command_does_not_see_ambient_secret` **and**
`inherited_shell_command_does_see_ambient_secret`. The pair distinguishes "the scrub works"
from "the canary was never set", which a single negative assertion cannot. `process/environment.rs:92`
does the same at unit level by exec'ing `/usr/bin/env` and asserting the canary is absent
from real child output rather than inspecting a `Command` struct.

**A boundary test needs the allow case asserted alongside the deny case, or it cannot fail
when the boundary silently stops applying.** This is the positive form of
`mistakes/tests-that-cannot-fail.md`.

## Degraded Modes Are Reported, Never Silent

`FileCoverage` gives every file a node even when it could not be parsed — `LexicalOnly`,
`Oversized`, `ParseError` — so a consumer can tell "no symbols here" from "not analysed".
When adding a new extractor or analyser, the degraded paths are the ones to test: the happy
path fails loudly, the degraded paths fail silently.

## Advisory Preflight: Do the Work, Report the Failure, Never Bail

`advisory_source_graph_preflight` (`commands/run/checks.rs:103-111`) is the second
instance of a shape worth copying, after `advisory_codex_lane_preflight`. The contract
is three rules and no more:

- it returns `()`, never a `Result`, so no caller can accidentally make it fatal;
- on failure it prints ONE `eprintln!` line with a stable prefix and swallows the error;
- it is idempotent and silent on the common path — `publish_source_graph`
  (`checks.rs:127-129`) early-returns when the layer for `HEAD` already exists.

Use it for derived state that IMPROVES a run but must never block one. The signature is
the enforcement: a function that cannot return an error cannot be made load-bearing by a
later caller who forgets it was optional.

## Spool-and-Drain: Writing Through a Sandbox You Cannot Widen

A stage agent's `.work` is a symlink into the main repo and the sandbox denies writes to
it, so `loom memory note` could not reach its own journal. Rather than widening the
sandbox, the write was made asynchronous:

- the agent appends to `<worktree_root>/.loom/memory-spool.jsonl` (`SPOOL_RELPATH`,
  `fs/memory/spool.rs:33`), size-capped at `SPOOL_MAX_BYTES` (1 MiB);
- `record()` (`commands/memory/handlers/record.rs:17`) falls into `record_via_spool`
  ONLY when the direct write failed AND `is_write_denied(&error)` matches
  `PermissionDenied`/EROFS — every other error still propagates unchanged;
- the daemon drains it: `Orchestrator::drain_stage_spools`
  (`orchestrator/core/spool_drain.rs:38`) every tick, plus a teardown drain
  `drain_spool_before_removal` (`git/cleanup/batch.rs:67`) so worktree-removal paths with
  no live orchestrator do not destroy pending entries.

Two design points to keep if you copy it. **The spool payload carries no stage id** — the
daemon attributes entries to the stage that owns the worktree it drained, so an agent
cannot forge another stage's journal (a real prompt-injection channel: a stage's journal
is quoted into that stage's later prompts). And **`drain_stage_spools` enumerates stages
by scanning `.work/stages/` on disk**, not from `active_worktrees`/`active_sessions`:
neither in-memory map survives a daemon restart, so disk is the only source of truth for
a stage recovered as still-Executing.

Known gap: with no daemon running at all, spooled entries stay pending until the next
tick or a teardown drain. `record_via_spool` says so in its own warning
(`record.rs:131`) rather than pretending the write landed.

## Ask Which Surfaces Render the Type, Not Who Copied the Helper

`context/untrusted.rs:5-8` names its call sites in a doc comment — "this has exactly two
call sites, do not add a third copy". That does not prevent a THIRD SURFACE from having
ZERO copies. `loom map` was rewritten into an agent-facing renderer of the same
graph-derived strings — scopes, paths, ids, and a `ParseError` detail built from a raw
line of the offending source file — and flattened none of them.

**Rule:** when a new command renders values an existing renderer flattens, the review
question is "which surfaces render this TYPE?", not "did anyone copy the helper?" — grep
for the type's FIELDS, not for the helper's name. And when you do flatten, route every
variant through it, not only the one that is currently attacker-controlled: uniform
treatment is free (`inline_safe` passes fixed-format strings through unchanged by its own
contract) and it avoids an asymmetry that will catch out whoever adds the next variant.
