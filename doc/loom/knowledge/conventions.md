# Coding Conventions

> Discovered coding conventions in the codebase.
>
> **Related files:** [patterns.md](patterns.md) for design patterns, [architecture.md](architecture.md) for system overview.

## File & Branch Naming

| Type           | Pattern                                                    | Location          |
| -------------- | ---------------------------------------------------------- | ----------------- |
| Stage files    | `{depth:02}-{stage-id}.md` (depth 0 = `01-` prefix)        | `.work/stages/`   |
| Session files  | `{session-id}.md` (ID: `session-{uuid_short}-{timestamp}`) | `.work/sessions/` |
| Signal files   | `{session-id}.md`                                          | `.work/signals/`  |
| Handoff files  | `{stage-id}-handoff-{NNN:03d}.md`                          | `.work/handoffs/` |
| Plan files     | `PLAN-*` -> `IN_PROGRESS-PLAN-*` -> `DONE-PLAN-*`          | `doc/plans/`      |
| Stage branches | `loom/{stage-id}`                                          |                   |
| Base branches  | `loom/_base/{stage-id}` (multi-dep merges)                 |                   |

## Error Handling

- All fallible functions return `anyhow::Result<T>`
- Chain context at each layer: `.with_context(|| format!("Failed to read: {}", path.display()))?`
- Git errors must include: command, directory, exit code, stdout, stderr

## Serialization

- State files use markdown with YAML frontmatter (`---` delimited)
- Serde: `#[serde(rename_all = "snake_case")]` on structs
- Use `#[serde(default)]`, `#[serde(skip_serializing_if = "Option::is_none")]`, `#[serde(alias = "...")]` as needed
- All timestamps: `DateTime<Utc>` from chrono

## Module Organization & Re-exports

Standard module layout: `mod.rs` (exports), `types.rs`, `methods.rs`, `transitions.rs` (if state machine), `tests.rs`

Re-export rules: `pub use` explicit items (never wildcards). Only export public API. `pub use` NOT `pub mod`.

## Testing

- Filesystem tests: `tempfile::TempDir` for isolation
- `#[serial]` from `serial_test` for tests needing exclusive access
- Naming: `test_<action>_<condition>`
- Inline `#[cfg(test)] mod tests {}` for simple cases; separate `tests.rs` for complex suites
- Integration tests in `loom/tests/integration/`, shared helpers in `helpers.rs`

## ID and Input Validation

| Field               | Rules                                                               |
| ------------------- | ------------------------------------------------------------------- |
| Stage ID            | Max 128 chars, `[a-zA-Z0-9_-]`, no `/\.`, no reserved OS names      |
| Fact Key            | Max 64 chars, `[a-zA-Z0-9_-]`                                       |
| Acceptance criteria | Max 1024 chars, no control chars (except tab/newline/CR), non-empty |

## Constants

```rust
// Context thresholds
DEFAULT_CONTEXT_LIMIT: u32 = 200_000;
CONTEXT_WARNING_THRESHOLD: f32 = 0.75;
CONTEXT_CRITICAL_THRESHOLD: f32 = 0.85;

// Timeouts
DEFAULT_COMMAND_TIMEOUT = 300s;
DEFAULT_VERIFICATION_TIMEOUT = 30s;
HUNG_SESSION_TIMEOUT = 300s;
POLL_INTERVAL = 5s;

// Retries
DEFAULT_MAX_RETRIES: u32 = 3;
BACKOFF_BASE_SECONDS: u64 = 30;
BACKOFF_MAX_SECONDS: u64 = 300;
```

## Display Conventions

Status icons: Completed=`✓` Executing=`●` Queued=`▶` WaitingForDeps=`○` Blocked=`✗` NeedsHandoff=`⟳` MergeConflict=`⚡` WaitingForInput=`?` Skipped=`⊘` CompletedWithFailures=`⚠` MergeBlocked=`⊗`

Colors (`colored` crate): Executing=blue.bold, Completed=green, Blocked=red.bold, Pending=dimmed, Queued=cyan, Warning=yellow

Context bar: <60%=green, 60-75%=yellow, >=75%=red

## Git Operations

```bash
git worktree add .worktrees/{stage-id} -b loom/{stage-id}
git worktree remove --force .worktrees/{stage-id}
git merge --no-ff -m "Merge loom/{stage-id}" loom/{stage-id}
git branch -D loom/{stage-id}   # Delete after merge
```

**Active-merge guard rule (2026-04-27):** Helpers that mutate git merge state (`merge_stage`, `get_conflicting_files_from_status`) MUST refuse via `require_no_active_merge` when `MERGE_HEAD` is set on the repo path. Never silently `git merge --abort`. Defense in depth: even if attribution misses an active merge upstream, the guard surfaces an error instead of corrupting in-progress resolution.

**Phantom-merge revert logging (2026-04-27):** All phantom-merge reverts (sync-time merged=true revert, daemon `reconcile_main_repo_active_merge`, CLI `RevertAndSpawnResolver`) MUST log at `tracing::error!` level — not `warn` — so they show up in production logs. Reverts represent invariants violated; the noise is the point.

## Plan YAML Schema

Required fields per stage: `id`, `name`, `working_dir` ("." or subdir), `dependencies` (list), `acceptance` (list)

Optional: `description`, `parallel_group`, `setup`, `files`, `auto_merge`, `stage_type` ("standard"|"knowledge"|"integration-verify"|"knowledge-distill")

Only `version: 1` supported.

## Enum Conventions

- Derive: `Debug, Clone, Serialize, Deserialize, PartialEq`
- Serde: `#[serde(rename_all = "kebab-case")]` for status enums
- Implement `Display` matching serde representation (e.g., `WaitingForDeps` -> `"waiting-for-deps"`)

## Builder Pattern

Used for complex struct construction: `fn builder() -> Self { Self::default() }` with `fn with_field(mut self, val) -> Self` chainable methods.

## Hook Conventions

- Location: `~/.claude/hooks/loom/`
- Naming: `<event>-<action>.sh` (e.g., `session-start.sh`, `post-tool-use.sh`)

## Comment Style

- Module docs: `//!` at top of file
- Function docs: `///` with `# Arguments`, `# Returns` sections
- Inline comments: sparingly, only for non-obvious logic

## Skill File Format

Directory: `skills/<skill-name>/SKILL.md`

Frontmatter fields: `name` (kebab-case, required), `description` (required), `triggers` (YAML array, highest priority), `trigger-keywords` (CSV string, fallback), `allowed-tools` (optional CSV).

Trigger priority: (1) triggers YAML array, (2) trigger-keywords CSV, (3) "TRIGGERS:"/"Trigger keywords:" in description text. Matching: phrase=2pts, word=1pt, threshold 2.0, max 5 per signal.

Body sections: Overview, When to Use, Instructions.

## Code Size Limits

File: 400 lines | Function: 50 lines | Struct impl: 300 lines | Exceed = refactor immediately

## Dependency Management

Never hand-edit manifests. Use: `cargo add`, `bun add`, `uv add`, `go get`

## Knowledge Files

Seven files: architecture, entry-points, patterns, conventions, mistakes, stack (aliases: deps, tech), concerns (aliases: debt, issues)

## Import Deduplication

When a pattern appears 3+ times, extract to a canonical location:

- `parse_stage_from_markdown` -> `verify::transitions::serialization`
- `branch_name_for_stage` -> `git::branch::naming` (never inline `format!("loom/{}", id)`)

## Signal File Format

Signal files at .work/signals/{session-id}.md use markdown with structured sections. Knowledge/merge/recovery signals have distinct formats. All share .work/signals/ directory.

## Map Module Conventions

Detectors skip: .git, .work, .worktrees, node_modules, target, .venv, **pycache**. Deep=3-level depth + concerns, Normal=2-level. Source extensions: .rs, .ts, .js, .py, .go, .java, .rb.

## Container Backend Conventions

- Embedded resources (Dockerfile.tmpl, firewall.sh, entrypoint.sh) live in `loom/resources/` and are accessed via `include_str!()` through `container/resources.rs`
- Backend type serializes as kebab-case: `"native"` / `"container"` (matches `BackendType` serde attribute)
- Container mount constants are named `<ROLE>_MOUNT` (all-caps), defined at the top of `container/mod.rs`
- `forward_credentials` default is `Vec::new()` (empty — explicit opt-in). Only add `"claude"` to forward `~/.claude/.credentials.json`. The mount is **not** the host file directly — `prepare_session_credentials` materializes a per-session writable copy at `<work_dir>/container-creds/<session_id>.json` and bind-mounts that rw at `/home/loom/.claude/.credentials.json`. Each container has its own OAuth refresh chain, isolated from the host and sibling containers. The copy is removed on `kill_session`. Other credential types (github, gitlab, ssh-agent) also supported.
- `permission_mode` YAML values are kebab-case: `"auto"`, `"accept-edits"`, `"bypass-permissions"`, `"plan"`, `"default"`
- `bypass-permissions` is only valid with `BackendType::Container` — `validate_config()` rejects it on native

## Resources Directory Convention

`loom/resources/` holds files that are embedded at compile time via `include_str!()`. These files:

- Are NOT installed to disk during `loom init` — they exist only inside the binary
- Are accessed through `container/resources.rs` helper functions
- Changes to these files automatically invalidate the image fingerprint (fingerprint hashes their content)
- `cargo build` must succeed with new resources before `loom container build` is tested

## Plan YAML Schema: Acceptance Field

The `acceptance` field in stage definitions uses `Vec<AcceptanceCriterion>` (not `Vec<String>`).
Two forms in YAML:

- Simple: `- "cargo test"` (plain string)
- Extended: `- command: "loom --help"\n  stdout_contains: ["Usage:"]` (object with TruthCheck fields)

`has_any_goal_checks()` checks ONLY: artifacts, wiring, wiring_tests, dead_code_check.
Validation requires: acceptance OR goal-backward checks for standard/integration-verify stages.

Old truths/truth_checks fields removed from StageDefinition. Serde silently ignores them in old plans (no deny_unknown_fields). before_stage/after_stage fields retained, still use TruthCheck.

## Hook Output Contract

Claude Code hooks communicate with the host process via stdin/stdout and exit codes.

**Exit codes:**

- `exit 0` — allow the operation to proceed (default, no output needed)
- `exit 2` — block the operation; stderr is shown to Claude as a `PreToolUse:` prefixed message
- Any other exit code — treated as an error (non-blocking, but logged)

**hookSpecificOutput (JSON response for warnings):**
To issue a warning without blocking (exit 0 with advisory), write a JSON object to stdout with a `hookSpecificOutput` field. Claude Code appends this to the tool result as additional context. Example:

```json
{"hookSpecificOutput": "LOOM_HOOK_WARN: consider using rg instead of grep"}
```

The `LOOM_HOOK_WARN:` prefix is recognized by the loom formatter (`render_tool_result` in `log_format.rs`) and rendered as `[hook warn]` in `loom container logs --format=human`.

**PostToolUse stdin schema:**

```json
{
  "tool_name": "Bash",
  "tool_input": {"command": "...", ...},
  "tool_result": {"output": "...", "is_error": false, "exit_code": 0},
  "session_id": "...",
  "session_info": {...}
}
```

Some fields may use `tool_response` instead of `tool_result` depending on Claude Code version — always use `(.tool_result.x // .tool_response.x)` patterns in shell hooks.

**PreToolUse stdin schema:** `tool_name` and `tool_input` fields only (no result yet).

**Stop hook (session end):** receives `{"reason": "...", "exit_code": N}`. Used by `commit-guard.sh` and `learning-validator.sh`.

## Dispute File Ownership Convention

`.work/disputes/<stage>/<n>/` — always split by authority:

- `request.md` — agent-attestable; written by daemon on behalf of agent RPC
- `verdict.md` — daemon-only; worker thread writes after API call
- `applied.marker` — daemon-only; zero-byte idempotency sentinel

Never collapse into one file — if agent can write the verdict section, it can self-approve.

## Admin Token Path Convention (After Stage 2)

Admin token moves OUT of `.work/` to a daemon-runtime-only path:

- Linux: `$XDG_RUNTIME_DIR/loom/admin.token` (via `dirs::runtime_dir()`)
- Fallback: `dirs::data_dir().join("loom/admin.token")`
- Mode: 0o600; created at daemon start, deleted at daemon stop

Container mounts are scoped to project root and `.work/` — the runtime dir is never mountable. This makes path-unreachable the actual security boundary, not file permissions.

## Adjudicator Scope Convention

The adjudicator amends ONLY:

- `acceptance: Vec<AcceptanceCriterion>` (plan/schema/types.rs:316)
- `wiring: Vec<WiringCheck>` (plan/schema/types.rs:336)

Never amends: `before_stage`, `after_stage`, `artifacts`, `dependencies`, `id`, `working_dir`, `model`, `sandbox`, `execution`. Use `AmendmentField` enum to enforce this at the type level.

## Dispute Budget Limits Convention

Per-stage caps to bound the autonomy loop:

- `dispute_count`: max 3 per stage (default)
- `evidence_rounds` (NeedsMoreEvidence iterations): max 2 before escalation to NeedsHumanReview
- `amendments_applied`: max 3 per stage (absolute, not percentage)
- `adjudicator_attempt_count` (worker crash retries): max 3

## .inflight Marker Convention

Worker threads write `.inflight` before starting HTTP call; delete on completion or handoff. Orchestrator main loop checks timestamp on each tick — if >10min old → re-fire worker (bounded by `adjudicator_attempt_count`). Pattern mirrors `.applying` markers from hooks.

## Daemon-as-Filesystem-Writer Convention

For any operation where agent data must be persisted to `.work/` with authority separation: the CLI (running inside container) sends RPC to daemon; the daemon (host-only) writes the file. The container has no rw mount to the target subdirectory. Examples:

- `loom memory note` → daemon writes `.work/memory/<id>.md`
- `loom stage dispute-criteria` (after Stage 2) → daemon writes `.work/disputes/<stage>/<n>/request.md`

## ANTHROPIC_API_KEY Access Convention

- Daemon process: reads from `std::env::var("ANTHROPIC_API_KEY")` directly (host env, no scrubbing)
- Container agents: key is scrubbed by `scrub_settings_env_for_backend()` in `sandbox/settings.rs:39-47`
- Absent key at daemon startup: adjudication disabled for that daemon run; disputed stages go directly to `NeedsHumanReview`
- Never pass the key to spawned sessions — it flows only to the daemon's adjudicator worker thread
