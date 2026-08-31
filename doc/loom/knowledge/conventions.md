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
// Context thresholds (models/constants.rs)
DEFAULT_CONTEXT_CEILING_TOKENS: u32 = 150_000;
DEFAULT_SUBAGENT_CEILING_TOKENS: u32 = 120_000;
MIN_CONTEXT_CEILING_TOKENS: u32 = 60_000;
DAEMON_CEILING_MULTIPLIER: f32 = 1.25;

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

`DEFAULT_CONTEXT_LIMIT`, `CONTEXT_WARNING_THRESHOLD`, `CONTEXT_CRITICAL_THRESHOLD`, `DEFAULT_CONTEXT_BUDGET`, `CONTEXT_ABSOLUTE_MAX` and the `display::CONTEXT_*_PCT` module were all deleted with the move from a percentage context budget to an absolute token ceiling — do not reintroduce them.

## Display Conventions

Status icons: Completed=`✓` Executing=`●` Queued=`▶` WaitingForDeps=`○` Blocked=`✗` NeedsHandoff=`⟳` MergeConflict=`⚡` WaitingForInput=`?` Skipped=`⊘` CompletedWithFailures=`⚠` MergeBlocked=`⊗`

Colors (`colored` crate): Executing=blue.bold, Completed=green, Blocked=red.bold, Pending=dimmed, Queued=cyan, Warning=yellow

Context bar: renders absolute `tokens`/`ceiling` and colours off `context_health(tokens, ceiling)` (`commands/status/render/progress.rs:56-72`) — Green `<60%`, Yellow `60-90%`, Red `>=90%` of the resolved ceiling, not a fixed percentage of a 200k window.

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

**`loom knowledge sync` (and anything that reaches `context::retrieve::resolve_roots` → `ContextStore::open` → a `refresh` write) can never sit in a worktree stage's acceptance list.** `ContextStore::open` (`context/store.rs:49`) resolves the context cache under `WorkDir::main_project_root().join(".loom/cache/context-v1")` — deliberately OUT of the worktree, through the `.work` symlink, to the MAIN repository, so parallel stages share one cache instead of each growing an immediately-stale private copy. `sync`'s `refresh` step (`context/refresh.rs:218`) then WRITES there via `ContextStore::save_catalog`, and both settings emitters strip `.loom` from `allow_write`, so that write always trips the sandbox from inside a worktree. `loom knowledge context` (retrieval) also opens the same store but is safe: its refresh failure downgrades to a warning and it builds the catalog in memory instead (`context/retrieve.rs:147-149`), which is why the signal footer tells agents to run it directly. `loom knowledge check` was written specifically to be safe as an acceptance criterion by NEVER opening the context store at all — it resolves only the knowledge root and calls the pure, read-only `catalog::build` (`commands/knowledge/check.rs:1-21`); do not "simplify" it back into `context::resolve()`.

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

| File | Authority | Notes |
| --- | --- | --- |
| `request.md` | agent-attestable | written by the daemon on behalf of the agent's RPC, or drained from the worktree spool |
| `verdict.json` | adjudication session | the session's DRAFT, not the record — it has no authority until recorded |
| `verdict.md` | daemon-only | the record `apply_pending_verdicts` reads |
| `applied.marker` | daemon-only | zero-byte idempotency sentinel |
| `attempts` | daemon-only | respawn budget, spent when an adjudication job is handed out |

Never collapse these into one file. The rule behind the split is unchanged: **if the party under
dispute can write the verdict, it can self-approve.**

What changed on 2026-08-31 is WHO writes the verdict, so the guard moved rather than disappearing.
The adjudicating session is a different session from the disputing stage agent, and it records
through `loom stage adjudicate`, which refuses:

- when `LOOM_WORKTREE_PATH` is set — a stage worktree session can never record a verdict, which is
  precisely the self-approval case;
- when the stage is not in `NeedsAdjudication`;
- when the named dispute was never filed;
- when a `verdict.md` already exists — a recorded verdict is not replaceable.

A degenerate verdict escalates rather than being recorded. An earlier version of this section said
`verdict.md` was written by "a worker thread after an API call"; there is no worker thread and no
API call.

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

## Adjudication Attempt Budget Convention

Respawn of an adjudication session is bounded by an on-disk `attempts` file in the dispute
directory, incremented when a job is handed out — the same shape as the merge-resolver attempt
counter. `MAX_ADJUDICATION_ATTEMPTS` is 3.

**The `.inflight` marker this section used to describe is gone**, along with the worker thread it
guarded. An adjudication session's liveness is read from the session registry
(`live_sessions_for_stage` filtered to `SessionType::Adjudication`, PID identity with start-time
verification), so there is no marker to go stale and no 10-minute freshness window to tune. That
also survives a daemon restart, which a process-local marker did not.

**A bug worth remembering, found when this was rebuilt:** the old attempt cap never fired once.
`current_attempt_count` derived the count by reading `verdict.md` — a file that by construction does
not exist on the branch where the cap is checked — so it always returned 0, and only the
`.inflight` marker's freshness limited respawn at all. A counter derived from an artifact that the
counted operation has not produced yet is not a counter. Spend the budget when the work is HANDED
OUT, not when it succeeds.

## Daemon-as-Filesystem-Writer Convention

For any operation where agent data must be persisted to `.work/` with authority separation: the CLI sends RPC to daemon; the daemon writes the file. Examples:

- `loom memory note` → daemon writes `.work/memory/<id>.md`
- `loom stage dispute-criteria` (after Stage 2) → daemon writes `.work/disputes/<stage>/<n>/request.md`

## Adjudicator Transport Convention

Adjudication runs in a **spawned loom session**, the same way merge conflict resolution does. There
is no API key, no HTTP call, and no headless subprocess anywhere in the path.

**Corrected twice on 2026-08-31, so read the history before reintroducing either shape.** This
section first described an `ANTHROPIC_API_KEY` gate: absent key at daemon startup disabled
adjudication for the whole run and every dispute escalated to `NeedsHumanReview` — which killed the
feature for anyone on a subscription rather than an API key. It was then briefly a headless
`claude -p` subprocess driven from a worker thread. Both are gone. The owner's call: a dispute is
judged by a real session with the full tool surface, because that is what lets it check the tree and
run things.

- **Spawn:** `adjudication::session::start_pending_adjudications` builds a `SessionType::Adjudication`
  session and spawns it through the same `TerminalBackend` every other session goes through. It runs
  in the MAIN REPO, not a worktree — `SessionType::Knowledge` is the closest precedent.
- **Model:** `resolve_model` reads `.work/config.toml::[adjudication].model`, defaulting to `opus`.
- **Briefing:** `orchestrator/signals/adjudication.rs` writes a signal whose body comes from
  `adjudication::prompt`, carrying the dispute, the stage's criteria, the evidence commit diff and
  the failure output.
- **The session RUNS the disputed criterion.** This is the point of using a session rather than a
  one-shot call. `prompt::execution_site` resolves the stage's worktree root joined with its
  `working_dir` — the same `EXECUTION_PATH` acceptance criteria resolve against — and the briefing
  tells the session to execute the criterion there and rank the observed exit code above the
  disputing agent's account of it. Running it from the wrong directory makes a sound criterion look
  broken and inverts the verdict, which is why the site is computed rather than guessed.
- **It judges, it does not fix:** no edits, no commits, never `loom stage complete`. The single
  authorized execution is the disputed criterion itself — the same command the stage already runs,
  under the same sandbox, so it is not new exposure.
- **Return path:** the session writes its verdict JSON to `verdict.json` in the dispute directory and
  runs `loom stage adjudicate --stage <id> --dispute <n> --verdict-file <path>`. The daemon's
  existing `apply_pending_verdicts` applies it on the next tick. Nothing waits on the session; the
  daemon observes the state change, exactly as it does for a merge resolution session.
- **Liveness and budget:** a live adjudication session is detected through
  `session_registry::live_sessions_for_stage` filtered to `SessionType::Adjudication` (PID identity
  with start-time verification), so it is correct across a daemon restart with no in-memory state.
  Respawn is bounded by an on-disk `attempts` counter in the dispute directory, spent when a job is
  handed out.
- `sandbox/settings.rs`'s `SENSITIVE_ENV_KEYS` still filters `ANTHROPIC_API_KEY` from agent sandbox
  environments. That remains correct hygiene and has nothing to do with adjudication.

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
- **Only the main agent runs `cargo` at all**, and it runs under a RAM watchdog rather than
  a job-count throttle. Measured on this 32-core machine, a full `cargo build --all-targets`
  peaks around 3 GB and the whole test suite barely moves the needle — so throttling `-j` is
  the wrong lever and just wastes the machine. What actually exhausted 125 GB was **leaked
  detached child processes**, not build parallelism (see
  [Never Spawn a Surviving Process From a Test](mistakes/detached-spawn-in-tests.md)).
  Run cargo wrapped in a watchdog that samples `free`, kills the whole **process group** on a
  low-headroom trip (killing cargo alone orphans the `rustc` children holding the pages), and
  reports any `loom` process still alive after exit.
- **A test may never create a process that outlives the test harness.** `cargo test` gives no
  warning for a leaked detached child; it simply exits green while the child keeps running.
  See [Never Spawn a Surviving Process From a Test](mistakes/detached-spawn-in-tests.md) —
  this cost a reboot once already.

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
`context/rank.rs` + `context/rank/{corpus.rs,rungs.rs}` do). **Never `<name>/mod.rs`**
— it deletes the path that a stage's artifacts and wiring lists pin. Check the ledger
and the wiring patterns first; see `mistakes/pinned-literals-ledgers-and-wiring.md`.

(Correction 2026-08-21: this section previously cited `context/graph_store.rs` +
`graph_store/` as the worked example, but the tree has never had a
`context/graph_store.rs` file — that module has always been `context/graph_store/mod.rs`,
the OTHER layout this section says to avoid. `context/rank.rs` is a verified, currently
accurate example of the sibling-style split with no `mod.rs`; other equally valid ones
include `context/lexical.rs` + `context/lexical/`, `context/refresh.rs` +
`context/refresh/`, and `context/retrieve.rs` + `context/retrieve/`. The
`<name>/mod.rs` layout is not wrong everywhere — `commands/hook/mod.rs` and
`context/graph_store/mod.rs` are both legitimate _directory modules_ built that way
from the start. The rule this section states applies specifically to _splitting an
existing top-level `<name>.rs` file_: converting it to `<name>/mod.rs` mid-split
changes the file's own path, which breaks anything pinning `<name>.rs` as a literal —
acceptance criteria, artifacts lists, wiring checks.)

A file that must ADD a wired submodule without editing a read-only parent (the file that
would otherwise gain the new `mod` declaration is owned by another stage or subagent) can
route around that instead of waiting: `#[path = "sibling_file.rs"] mod name;` inside the
file you DO own declares a flat sibling file as a child module, without any edit to the
directory's own `mod.rs`/parent declaration. `commands/hook/user_prompt.rs`'s
`#[path = "tests_user_prompt_e2e.rs"] mod e2e;` is the established precedent for this,
used for splitting a same-directory test file the same way.

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

```bash
cargo test --all-targets --no-fail-fast
```

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

## Bump `INDEX_VERSION` Whenever `lexical::tokenize` Changes

`context/lexical_index.rs::INDEX_VERSION` (currently `1`) has no compile-time
protection tying it to the tokenizer. The persisted index file already hashes
the `WEIGHT_*` scoring constants (`derivation()`, `lexical_index.rs:85-98`) and
is rejected on a mismatch, so retuning a weight cannot leave a warm cache
scoring at the old value. `lexical::tokenize` (`context/lexical.rs`) is the one
document input to that same index with **no** constant hashed into it: a
tokenizer change that keeps every source byte identical still changes what
each document's `(term, weight)` pairs are, and the index has no way to detect
that on its own.

The failure mode is a divergence visible only on a cache HIT: a warm index
built under the old tokenizer keeps serving old-tokenization postings, while a
cold miss rebuilds under the new tokenizer and scores differently — same code,
same corpus revision, two different answers depending on nothing but whether a
cache file happened to survive. See [Context Retrieval](architecture/context-retrieval.md)
for the rest of the index's invalidation contract (why `average_length` and
the document-frequency map are recomputed rather than stored, and why weights
are persisted as IEEE-754 bits).

**Rule:** any change to `lexical::tokenize` (the split rules, the emitted
casing, what counts as a token boundary) MUST bump `INDEX_VERSION` in the same
commit. A file at the wrong version is treated as a miss, not an error — the
reader falls back to the scan and rewrites the file — so bumping the version
costs nothing but a few extra scans on the next prompt per revision, while
forgetting it costs a silent, hit-only scoring divergence that no test
currently pins.
