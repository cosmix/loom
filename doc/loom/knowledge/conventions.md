# Coding Conventions

> Discovered coding conventions in the codebase.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [patterns.md](patterns.md) for design patterns, [architecture.md](architecture.md) for system overview.

## File & Branch Naming

| Type | Pattern | Location |
|------|---------|----------|
| Stage files | `{depth:02}-{stage-id}.md` (depth 0 = `01-` prefix) | `.work/stages/` |
| Session files | `{session-id}.md` (ID: `session-{uuid_short}-{timestamp}`) | `.work/sessions/` |
| Signal files | `{session-id}.md` | `.work/signals/` |
| Handoff files | `{stage-id}-handoff-{NNN:03d}.md` | `.work/handoffs/` |
| Plan files | `PLAN-*` -> `IN_PROGRESS-PLAN-*` -> `DONE-PLAN-*` | `doc/plans/` |
| Stage branches | `loom/{stage-id}` | |
| Base branches | `loom/_base/{stage-id}` (multi-dep merges) | |

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

Re-export rules in `mod.rs`:

- Declare submodules, then `pub use` explicit items (never wildcards)
- Only export public API items; keep helpers private
- `pub use` NOT `pub mod` for re-exports

## Testing

- Filesystem tests: `tempfile::TempDir` for isolation
- `#[serial]` from `serial_test` for tests needing exclusive access (git ops, etc.)
- Naming: `test_<action>_<condition>` (e.g., `test_transition_from_executing_to_completed`)
- Inline `#[cfg(test)] mod tests {}` for simple cases; separate `tests.rs` for complex suites
- Integration tests in `loom/tests/integration/`, shared helpers in `helpers.rs`
- Tests should not panic on missing tools; check graceful handling

## ID and Input Validation

| Field | Rules |
|-------|-------|
| Stage ID | Max 128 chars, `[a-zA-Z0-9_-]`, no `/\.`, no reserved OS names |
| Fact Key | Max 64 chars, `[a-zA-Z0-9_-]` |
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

## Plan YAML Schema

Required fields per stage: `id`, `name`, `working_dir` ("." or subdir), `dependencies` (list), `acceptance` (list)

Optional: `description`, `parallel_group`, `setup`, `files`, `auto_merge`, `stage_type` ("standard"|"knowledge"|"code-review"|"integration-verify")

Only `version: 1` supported.

## Enum Conventions

- Derive: `Debug, Clone, Serialize, Deserialize, PartialEq`
- Serde: `#[serde(rename_all = "kebab-case")]` for status enums
- Implement `Display` matching the serde representation (e.g., `WaitingForDeps` -> `"waiting-for-deps"`)

## Builder Pattern

Used for complex struct construction: `fn builder() -> Self { Self::default() }` with `fn with_field(mut self, val) -> Self` chainable methods.

## Hook Conventions

- Location: `~/.claude/hooks/loom/`
- Naming: `<event>-<action>.sh` (e.g., `session-start.sh`, `post-tool-use.sh`)
- See [patterns.md](patterns.md) for hook event pipeline and input/response patterns

## Comment Style

- Module docs: `//!` at top of file
- Function docs: `///` with `# Arguments`, `# Returns` sections
- Inline comments: sparingly, only for non-obvious logic

## Skill File Format

Directory: `skills/<skill-name>/SKILL.md`

Frontmatter fields: `name` (kebab-case, required), `description` (required), `allowed-tools` (optional CSV), `trigger-keywords` (optional CSV), `triggers` (optional YAML list)

Body sections: Overview, When to Use, Instructions

## Code Size Limits

File: 400 lines | Function: 50 lines | Struct impl: 300 lines | Exceed = refactor immediately

## Dependency Management

Never hand-edit manifests. Use: `cargo add`, `bun add`, `uv add`, `go get`

## Knowledge Files

Seven files: architecture, entry-points, patterns, conventions, mistakes, stack (aliases: deps, tech), concerns (aliases: debt, issues)

## Import Deduplication

When a pattern appears 3+ times, extract to a canonical location and import. Key canonical locations:

- `parse_stage_from_markdown` -> `verify::transitions::serialization`
- `branch_name_for_stage` -> `git::branch::naming` (never inline `format!("loom/{}", id)`)
