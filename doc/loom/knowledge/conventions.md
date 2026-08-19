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

- Application and orchestration code returns `anyhow::Result<T>` and adds actionable context when
  crossing a layer or performing an operation whose raw error does not identify the target.
- Use a typed error only when callers must distinguish domain outcomes by variant; do not erase that
  structure merely for visual uniformity.
- Preserve native adapter errors at their natural boundary, including `io::Result`, serde errors,
  `FromStr::Err`, and Clap validator strings. Convert them when application code consumes them.
- Git errors must include the command, directory, exit code, stdout, and stderr.
- Do not add a second general error framework without a concrete caller-facing API need.

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

`cargo test --test maintainability` enforces the file and function limits in CI. Legacy production
exceptions are recorded in `loom/maintainability-baseline.txt` and may only shrink: a new exception
or an increase above the recorded size fails the gate.

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

## Permission Mode YAML Values

`permission_mode` YAML values are kebab-case: `"auto"`, `"accept-edits"`, `"plan"`, `"default"`

## Plan YAML Schema: Acceptance Field

The `acceptance` field in stage definitions uses `Vec<AcceptanceCriterion>` (not `Vec<String>`).
Two forms in YAML:

- Simple: `- "cargo test"` (plain string)
- Extended: `- command: "loom --help"\n  stdout_contains: ["Usage:"]` (object with TruthCheck fields)

`has_any_goal_checks()` checks ONLY: artifacts, wiring, wiring_tests, dead_code_check.
Validation requires: acceptance OR goal-backward checks for standard/integration-verify stages.

Old `truths`/`truth_checks` fields were removed from `StageDefinition` and are now rejected by strict deserialization. `before_stage`/`after_stage` remain supported and still use `TruthCheck`.

Plan deserialization is strict at every policy-bearing layer: the metadata root, `LoomConfig`, `StageDefinition`, and nested sandbox, filesystem, network, Linux, adjudication, code-review, truth-check, wiring-test, and dead-code structures use `deny_unknown_fields`. A typo or retired field must fail parsing with an actionable unknown-field error; it must never disappear before validation. In particular, top-level `truths` is rejected.

## Hook Output Contract

Claude Code hooks communicate with the host process via stdin/stdout and exit codes.

**Exit codes:**

- `exit 0` — allow the operation to proceed (default, no output needed)
- `exit 2` — block the operation; stderr is shown to Claude as a `PreToolUse:` prefixed message
- Any other exit code — treated as an error (non-blocking, but logged)

**hookSpecificOutput (JSON response for warnings):**
To issue a warning without blocking (exit 0 with advisory), write a JSON object to stdout with a `hookSpecificOutput` field. Claude Code appends this to the tool result as additional context. Example:

```json
{ "hookSpecificOutput": "LOOM_HOOK_WARN: consider using rg instead of grep" }
```

The `LOOM_HOOK_WARN:` prefix is recognized by the loom hook system and surfaced as a warning in output.

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

For any operation where agent data must be persisted to `.work/` with authority separation: the CLI sends RPC to daemon; the daemon writes the file. Examples:

- `loom memory note` → daemon writes `.work/memory/<id>.md`
- `loom stage dispute-criteria` (after Stage 2) → daemon writes `.work/disputes/<stage>/<n>/request.md`

## ANTHROPIC_API_KEY Access Convention

- Daemon process: reads from `std::env::var("ANTHROPIC_API_KEY")` directly (host env)
- Absent key at daemon startup: adjudication disabled for that daemon run; disputed stages go directly to `NeedsHumanReview`
- Never pass the key to spawned sessions — it flows only to the daemon's adjudicator worker thread

## Vendored Agent Assets Live at Repo Root

Claude slash commands and Codex skills shipped by loom live in source at the repo root, NOT under `loom/`:

- `commands/*.md` — Claude slash commands (installed to `~/.claude/commands/`)
- `codex/skills/<name>/SKILL.md` — Codex skills (installed to `~/.codex/skills/<name>/`)

`install.sh` asserts the required source files exist before copying and fails the install if any are missing.

## Guidance Delivery Channels Convention

Agent guidance lives in the channel that delivers it closest to the decision point, cheapest:

- **Hooks** (`hooks/*.sh`) — rules that must never be violated (plans path, all-files staging, commit/complete, worktree isolation, subagent verification). Deterministic; the exit-2 message re-injects the rule at the exact moment of violation.
- **Stage signals** (`orchestrator/signals/`) — stage-execution mechanics (completion checklist, adversarial review dimensions). Delivered per-stage at execution time.
- **Skills** (`skills/*/SKILL.md`) — task-scoped expertise loaded on demand (`loom-plan-writer` owns ALL plan-authoring mechanics: YAML, working_dir, acceptance design, model selection, parallelization).
- **CLAUDE.md.template** — only cross-cutting rules and the 6-item hard-stop tier (stated verbatim at top AND bottom; middle of a long file is a retrieval dead zone). Do not restate what a hook, signal, or skill already delivers — duplicated guidance drifts and dilutes.

When adding new guidance, pick the channel first; the template is the channel of last resort.

**Hook versus prose — how to choose (2026-07-28).** Prose is advice an agent may reason its way
around; a hook is a wall. Escalate a rule to a hook when _all_ of these hold:

1. The violation is **cheap to detect mechanically** — a command shape, a path, a file state.
2. The violation is **expensive or irreversible** once it happens (lost work, corrupted state,
   a security relaxation granted wrongly).
3. Prose has **already failed**, or the rule contradicts a stronger instinct the agent has.

The plans-path rule is the worked example: it was the one hard rule with no hook enforcement and
it kept being violated, because "write the plan where you were told" loses to the harness's own
suggestion. Adding `plans-path-guard.sh` ended it.

Two obligations come with choosing the hook channel:

- **The prose does not go away — it must AGREE.** An enforcement layer landed without updating
  the guidance layer produces surfaces that actively instruct the blocked behaviour, and the
  agent obeys the instruction and hits a wall it was told to walk into. After adding a hook,
  sweep every prose surface for wording the hook now retires.
- **The refusal message is the guidance.** It is read at the exact moment of the mistake, so it
  must state the rule, the allowed alternative, and the carve-out — not just "blocked".

Corollary for exceptions: an exception must live in **every block that gets copied into a
subagent prompt**, not only in the prose that explains the rule.

## Verification Is the Main Agent's Job

Subagents do not verify. A subagent may run **at most one narrowly-scoped check** relevant to
the files it just changed; project-wide builds, full test suites, and repo-wide lint or
typecheck runs belong to the main agent, which is the only party that can see the whole tree.

Enforced by `hooks/subagent-verify-guard.sh` (PreToolUse:Bash), stated in the Rule 5 subagent
preamble in `CLAUDE.md.template`, and injected into stage signals by
`orchestrator/signals/cache.rs`. The three copies are pinned byte-for-byte by
`orchestrator/signals/tests_doctrine.rs`.

**The one exception:** an `integration-verify` stage exists to run the complete suite, so its
subagents are carved out. The carve-out is resolved from the stage file and **fails safe** — more
than one glob match, a non-integration-verify stage type, or a missing file all mean "no
relaxation".

## Git Push Requires Explicit User Request

Never `git push` unless the user explicitly asks — commit locally and stop. "Fix the CI failure" does NOT imply pushing to make CI green; the user decides when commits leave the machine. (Learned 2026-07-22: pushed after fixing a red CI run on the theory that CI-green was the deliverable — user rejected: "i didn't ask you to push.")

## Claude Code Plugin Scope in Loom Repos (2026-08-07)

Enable Claude Code plugins at **user or project scope** — `claude plugin install codex@openai-codex --scope user`.
Scope decides the file: user → `~/.claude/settings.json`, project → `.claude/settings.json`,
local → `.claude/settings.local.json`. That last file is the one loom REBUILDS from scratch on every
stage spawn (`sandbox::write_settings`, `sandbox/settings.rs:77`, called from
`orchestrator/core/stage_executor.rs:373` and `:584`, and from `loom repair --fix`).

**Nuance that shipped 2026-08-07 — do not over-read the old "local scope vanishes" rule.**
`preserve_unowned_keys` (`sandbox/settings.rs:587`) now carries a two-key allowlist across every
regeneration:

```rust
const PRESERVED_SETTINGS_KEYS: [&str; 2] = ["enabledPlugins", "extraKnownMarketplaces"];
```

Plugin enablement at local scope therefore _survives_ — verified by driving the real `write_settings`
over a seeded file twice. The user/project rule still stands, for a different reason than before:
the carve-out is exactly two keys, so local scope is safe **only** for plugins and **only** by
special case. Everything else you put in that file (`env`, custom `hooks`, ...) is still dropped on
the next spawn — see [Sandbox & Settings](mistakes/sandbox-and-settings.md).

Verify inside a worktree after a stage starts; never assume:

```bash
rg -n "enabledPlugins" .claude/settings.local.json
claude plugin list --json
```

## Additive Schema Fields: Prefer `#[serde(default)]` Over Bespoke Migration (2026-08-07)

For a new additive stage field, `#[serde(default)]` carries existing plan files and in-flight
`.work/stages/*.md` without a bespoke upgrade pass. This compatibility rule does not make removed or
misspelled plan fields permissive: strict plan structs still reject unknown fields. The shape used
for `implementers` and `subagent_timeout_secs`:

- Plan schema `StageDefinition` (`plan/schema/types.rs`) — `#[serde(default)]`; add
  `skip_serializing_if = "Option::is_none"` for `Option` fields so re-serialized plans stay clean.
- Runtime `Stage` (`models/stage/types.rs`) — `#[serde(default)]`, so a stage file written
  before the field existed still loads mid-run.
- Prefer a **closed enum over a string**: `Implementer` (`models/stage/types.rs:135`) derives
  `Default` + `#[serde(rename_all = "kebab-case")]`, so a typo is a parse ERROR (`unknown variant
'bogus-lane', expected 'claude' or 'codex'`), never a silent fallback. Pin `Display` to the serde
  spelling with a test (`plan/schema/tests/implementer_tests.rs`).
- Propagate along the existing chain — see [Adding New Plan Fields Checklist](architecture.md):
  plan → canonical `Stage::from_definition` (`models/stage/methods.rs`) → signal
  `EmbeddedContext` (`orchestrator/signals/generate.rs`).

**Model a per-stage capability as a SET, not a scalar, when more than one value can be true at
once.** `implementer` shipped as a single enum, which silently asserted that every subagent in a
stage came from one lane; real stages mix codex and Claude subagents. The fix was `Implementers`, a
`#[serde(transparent)]` newtype over `Vec<Implementer>` with `Default = [Claude]`, where MEMBERSHIP
licenses a lane and ORDER picks the default for routine work. Before adding an enum-valued stage
field, ask whether a stage could legitimately want two of its values simultaneously — if yes, ship
the list on day one, and gate any safety doctrine on `contains`, never on equality with the
preferred value. Validation must then reject the two shapes a list admits and a scalar could not:
the empty list and a repeated element.

**Guard the default with the `implementer_defaults` pair** — copy these two tests verbatim in shape
for any new field (`loom/tests/integration/implementer_defaults.rs`):

1. `*_plan_yaml_without_field_*` — parse plan markdown whose YAML has NO such key; assert every
   stage gets the default.
2. `*_stage_file_without_field_*` — write `.work/stages/*.md` frontmatter with no such key, call
   `load_stage()`, assert it loads and defaults.

Schema-only tests are not enough: they never touch the state files already on disk, which is exactly
where a non-defaulted field breaks a running plan.

## Formatting and Test Invocation in a Shared Worktree

- **Never run `cargo fmt` while sibling subagents are live.** `cargo fmt -- <path>`
  **IGNORES its path arguments** and formats the ENTIRE crate, silently reformatting files
  another agent owns — which shows up in `git status` as an ownership violation and can
  collide with an in-flight edit. Use `rustfmt --edition 2021 <file>` for your own files.
  Only the main agent runs repo-wide `cargo fmt`, and only after every subagent has landed.
- **`cargo test` accepts exactly ONE testname filter.** Extra filters are rejected with
  "unexpected argument" BEFORE compiling, so zero tests run. Use one common prefix
  (`cargo test --lib context::`) or separate invocations chained with `&&`.
- **Filter by the real module path.** Tests under a `tests` submodule need it spelled out:
  `context::tests::delivery`, not `context::delivery`, which matches nothing.
- **`rustfmt`'s `fn_call_width = 60` is what forces a call vertical, not `max_width = 100`.**
  Sum the argument names including `,` separators; over 60 and rustfmt goes one-arg-per-line,
  which can explode a match arm and trip the 50-line function gate. Renaming in the pattern
  (`budget_tokens: budget`) is a legitimate way back under the limit.

## Dependency Pins for Native-Grammar Crates

Exact-pin (`=x.y.z`) any dependency whose generated output is cached, and collapse a family
of optional deps behind ONE feature rather than letting `cargo add` mint an implicit feature
per dep — otherwise a host can disable half a family and leave a registry inconsistent. See
`architecture/source-graph.md` for the worked example.

**After any `cargo add` that pulls a new crate, run `cargo fetch` ONCE with the sandbox
disabled.** The Bash sandbox makes `~/.cargo/registry/cache` read-only, so a later
`cargo build` dies with `failed to open .../<crate>.crate: Read-only file system (os error 30)`
— which reads like a corrupt registry rather than a permissions problem. Every build after
that fetch works inside the sandbox because the `.crate` files are present.

## Splitting a File

Use the edition-2021 layout `<name>.rs` plus a `<name>/` subdirectory (as
`context/graph_store.rs` + `graph_store/` do). **Never `<name>/mod.rs`** — it deletes the
path that a stage's artifacts and wiring lists pin. Check the ledger and the wiring patterns
first; see `mistakes/pinned-literals-ledgers-and-wiring.md`.

Two visibility details that bite when splitting:

- Re-export across a module boundary explicitly: an item marked `pub(crate)` is unreachable
  if any module on its path is declared without `pub`.
- Moving a `#[cfg(test)]` fixture DEEPER breaks an existing `pub(super)` re-export (E0364).
  Give the moved item `pub(in crate::path::to::original::scope)` to match its original
  effective visibility exactly, rather than `pub(super)`.

## Docstring Honesty

State the current wiring, not the intended one. If a consumer is unbuilt, say so — the house
style to copy is `commands/context/record_edit.rs:12-14`, which states outright that it is
consumed by nothing and is pure input for a consumer that has not been built. Prefer intra-doc
links (`` [`crate::context::refresh`] ``) over plain backticks for module cross-references, so
a wrong path becomes a rustdoc warning instead of a permanent lie that survives
`clippy -D warnings`.

## Deliberately-Invalid Test Fixtures

`tests/maintainability/scanner.rs` parses EVERY `.rs` file under the crate, `tests/fixtures/`
included, and errors on unbalanced braces. An intentionally-unparseable fixture must therefore
**not carry a real `.rs` extension** — name it `<name>.rs.broken` and pass a virtual `.rs`
dispatch path to whatever must still treat it as that language.

## Working Directory

The Bash tool keeps its working directory ACROSS calls in this harness, so one `cd` silently
redirects every later relative command. Prefix verification commands with an explicit absolute
`cd`, or pass absolute paths. **If EVERY independent check fails at once — fmt and clippy and
build and an unrelated gate — suspect the working directory before the code**; a real
regression almost never breaks all of them in the same instant.

## The Maintainability Ledger Is Shared State, and Only One Concurrent Stage May Own It

`loom/maintainability-baseline.txt` is an EXACT-match ledger: it fails when the code
SHRINKS as well as when it grows, and a plain `cargo test` runs it. It is also one
file at one path, shared by every worktree in a plan.

Three consequences a plan author has to design around:

1. **Exactly one CONCURRENT stage may own the ledger.** Two parallel stages that both
   grow or delete ledgered code will conflict on merge, and each will have reconciled
   against a baseline the other invalidated.
2. **A plan that grows or deletes ledgered code without owning the ledger cannot pass
   its own acceptance.** Deleting a ledgered function fails exactly like adding an
   over-long one, so a stage that removes ~4000 lines of orphaned surface MUST also
   hold the ledger.
3. **When a refactor drops an entry under the limit, DELETE the entry rather than
   lowering it.** Lowering keeps a permanent claim on a function that no longer needs
   one.

Before adding lines to any function: `rg '<fn name>' loom/maintainability-baseline.txt`.
If it is listed, refactor rather than extend.

## `cargo test` Is Not This Repo's Test Gate — `--all-targets --no-fail-fast` Is

Never write plain `cargo test` into a loom plan's acceptance criteria. The gate is:

    cargo test --all-targets --no-fail-fast

Both flags earn their place:

- **`--all-targets` is what compiles `loom/tests/**`.** Without it the external
  integration tests are never built, so a changed signature breaks them and NOTHING
  reports it until somebody runs the full command by hand. Signature changes are
  exactly what a refactor stage produces, which is where this bites hardest.
- **`--no-fail-fast` is what makes the report exhaustive.** Stopping at the first
  failing target hides how much else is red; an agent then fixes one failure, re-runs,
  and discovers the next — one round trip at a time.

Know the two non-hermetic tests, so a red run inside a stage session is not
misdiagnosed as your own breakage. The stage-finalisation tests
(`commands/stage/tests/complete.rs`) route through `sandbox_control_session`
(`control_session.rs:70,94`), which reads `LOOM_STAGE_ID` / `LOOM_SESSION_ID` /
`LOOM_WORKTREE_PATH` from the ambient process environment. Running the suite from
INSIDE a loom worktree session leaves those set, silently routing the call down the
sandboxed worktree path instead of the host-side one the test means to exercise, and
it fails with a wrapper-identity mismatch. It is also order-dependent: it failed in
one full `--all-targets` run and passed in the next.

Re-run with `env -u LOOM_STAGE_ID -u LOOM_SESSION_ID` BEFORE concluding your change
broke it. The durable fix is an RAII env guard at test start — mirroring `EnvGuard`
in `commands/memory/handlers/tests.rs` — that restores on `Drop`, so a panic mid-test
cannot leak state into later tests. Do not apply that fix from an unrelated stage:
touching a file outside your territory is cross-stage merge-conflict bait.
