# Briefs And Bug Reports

> Stage bug reports; guard flags in briefs

## A Bug Report From a Loom Stage Is About Loom the Product, Not About a Project on This Machine (2026-09-18)

**What happened:** the user relayed a `knowledge-distill` stage's report of a relay-ticket deadlock. The investigation searched `~/.claude/projects` for the error string and started reading transcripts belonging to another project's worktree. The user stopped it: the reporting instances ran in other projects that use loom as a product, and those projects are out of reach.
**Why:** the report was treated as a local incident to reconstruct from logs, when it was a defect report against loom's source.
**Prevention:** a report quoted from a stage session is a symptom description. Diagnose it from loom's own source, tests, and knowledge in this repo; never go looking for the reporter's transcripts, worktrees, or state, and never read another project's directory under `~/.claude/projects`. Ask the user for more detail from the report if the source leaves it ambiguous.
**Fix:** drop anything learned from the other project's files and reason from the code path the quoted error names.

## A Brief That Invents a New Guard Flag Can Widen an Existing One (2026-09-18)

**What happened:** the brief for the relay hook's new ` memory ` fast-path arm told the worker to add a `NOTIFY` flag (relay line OR persisted-output marker) and gate every diagnostic on it. The worker did, moving `say()` off `HAS_LINE`. `say()` had been on `HAS_LINE` alone on purpose ("a large output that was merely persisted to a file is too common to comment on"), so any large Bash output could now emit relay diagnostics. Caught in the orchestrator's diff review; reverted in a fix pass.
**Why:** the brief specified a mechanism without first checking whether the existing guard already gave the required silence. It did: a payload admitted only by the new arm has `HAS_LINE=0`.
**Prevention:** before a brief introduces a new condition variable next to an existing one, state in the brief what the existing one already covers and why it is insufficient. If that sentence cannot be written, the new variable is not needed. A worker's "I changed behaviour X, flagging in case" note is a review item, never a footnote.

## A Lint That Newly Rejects a Command Shape Needs a Fixture Sweep Before the Brief (2026-09-19)

**What happened:** the fix brief for the masked-exit lint said to flag a final statement that is exactly
`true`, `:` or `exit 0` even with nothing before it. 39 tests (`plan::tests::amendment*`, the e2e
`criteria_validation` suite, the integration `plan_verify` suite) use a lone `true` as a placeholder
criterion or wiring test and failed validation. The worker's one allowed check (`--lib plan::schema`) could
not see them.
**Why:** the brief was written from the lint's design, not from the shapes the repository's own fixtures use.
**Prevention:** before a brief makes a validator reject a shape it accepted, `rg` the fixtures for that exact
shape (`rg -n '"true"' loom/src loom/tests`) and put the count in the brief. A lone no-op is a masked exit only
when a real command precedes it.
**Fix:** `masks_exit_status` requires an earlier non-empty statement for the bare forms (see
[plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md)).

## Adding a Struct Field: Search src AND tests, and Gate With `--all-targets` (2026-09-19)

**What happened:** a brief scoped the `StageDefinition` literal sites with `rg -l 'StageDefinition {' loom/src`
and missed five literals under `loom/tests/` (`integration/helpers.rs:92`, `e2e/criteria_validation/mod.rs:24`,
`e2e/daemon_config/mod.rs:65`, `tests.rs:65` and `:101`). The worker's `--lib` build passed; `--all-targets`
failed.
**Prevention:** when adding a field to a struct built by exhaustive literals, search the whole `loom/` crate,
not `loom/src`, and do not trust a worker's lib-only check: the orchestrator gates with `--all-targets`.

## Narrowing a Lint Must Name the True Positives That Still Fire (2026-09-19)

**What happened:** to kill a false positive the review-fix brief told a worker to restrict `HomeFromExpansion`
to the command's prefix words and "not widen scope". The worker dropped detection of `export HOME=$x` (and
deleted its test assertion) because `command_start` never classifies `export` arguments as prefix.
**Prevention:** a brief that narrows a lint lists the true positives that must keep firing (`export`,
`declare`, `local`, `readonly`, `typeset` arguments, an `env` prefix) and forbids removing an existing
assertion.

## The Maintainability Baseline Is a Ratchet: a Brief Must Say So (2026-09-19)

**What happened:** three separate workers broke it. Worker K2 raised `loom/maintainability-baseline.txt`
entries (`cache.rs` 529 to 530, `generate_knowledge_distill_stable_prefix` 79 to 80) to fit a one-line
addition and pushed `memory/handlers/tests.rs` to 424 lines. A sonnet worker told to offset baseline growth
deleted blank `///` doc separators (clippy `doc_lazy_continuation` at `models/stage/types.rs:670`) and split
`let now` into two `Utc::now()` calls in `models/stage/defaults.rs`, so `created_at != updated_at`.
**Why:** the briefs did not say the baseline may only shrink, and "offset the growth" reads as permission to
squeeze lines.
**Prevention:** every brief that touches an oversized function or a test file near 400 lines states that
`maintainability-baseline.txt` may never be raised and that new tests go in a sibling `tests_*.rs`. A
baseline-offset brief forbids deleting doc separators, merging statements that change behaviour and joining
lines; it requires a real extraction (move a cohesive block to a sibling module) and lowering the entry.
**Fix:** K2's work was re-delegated to refactor net-zero and split its tests.
**Known residue:** `Stage::default` (`models/stage/defaults.rs:6`) is the single exhaustive `Stage` literal and
sat exactly at its baseline (70), so every new `Stage` field grows it by a line with no in-function reduction
available. The plan-verification stage raised its entry to 71 by an explicit, recorded decision. Any stage
that adds a `Stage` field needs either that decision or a field-grouping refactor planned in advance.

## Read the Criterion Direction From the Plan, Not From the Signal (2026-09-19)

**What happened:** spawn prompts told two workers that the plan-writer `rg -qF '## '` and agents
`tools:.*Task` criteria must MATCH; the plan sets `exit_code: 1` for both (they must be ABSENT).
**Why:** the signal's Acceptance Criteria list renders only `criterion.command()`
(`signals/format/sections.rs:485`), so a criterion that must fail reads as a positive check.
**Prevention:** read the stage's YAML `acceptance` (with `exit_code`) in the plan before briefing; never infer
the direction from the signal's bare command list. The rendering gap is recorded in
[code-quality-and-hook-debt](../concerns/code-quality-and-hook-debt.md).

## `loom-code-reviewer` Is Read-Only, So the Orchestrator Runs the Gate for It (2026-09-19)

**What happened:** an integration-verify stage spawned reviewers by agent type. `loom-code-reviewer` has only
`Read`, `Glob` and `Grep`: no Bash, no `Skill`, no git. It could not run the canonical gate, load
`loom-security-audit`, or read a diff itself.
**Prevention:** the orchestrator runs the gate, writes each reviewer's diff to the session scratchpad, and names
the skill's `SKILL.md` path so the reviewer can `Read` it. Do not brief a reviewer with commands it cannot run.
