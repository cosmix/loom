# Pinned Literals Ledgers And Wiring

> The maintainability ledger exact-match trap and goal-backward wiring checks pinning a pattern to a path.

## Why These Two Belong Together

Loom pins two different things to literal values, and both punish an otherwise-correct
refactor. Three stages of one plan hit the ledger and two hit the wiring pins, so
neither is an edge case: they are the standing tax on editing this repo. The unifying
rule is **check the pins before you fan out, and re-check them after every refactor
round** — not at commit time, when it means reopening every worker diff.

## The Maintainability Ledger Is EXACT-Match, Not a Ceiling

`loom/maintainability-baseline.txt`, enforced by `cargo test --test maintainability`
(`tests/maintainability/baseline.rs::validate_recorded_entries`).

**It errors on SHRINKAGE exactly as loudly as on growth.** "shrank from X to Y" is a
failure. So deleting a line from an over-limit function fails the gate until its entry
is lowered, and growth of a ledgered entry is never recordable — only refactoring
clears it. Stale entries are rejected too.

Consequences that bit real work:

- A file recorded at its exact count cannot take **one** more line. `cli/types.rs` sat
  at exactly 400/400, so adding a single clap variant was a new file violation.
  `merge_handler.rs` carried a 1430-line entry, so routing two cleanup call sites
  through a new primitive was only safe because the file ended up NET SHORTER.
- A ledgered FUNCTION entry (`dispatch.rs dispatch 205`) means the top-level match
  cannot take even one more arm.
- **The two size caps fight each other.** Extracting a helper to get a function under
  the 50-line cap pushed `refresh/source_graph.rs` to 407 against the 400-line file
  cap. Keep the new helper's doc comment tight rather than padding with blank lines.
- In a shared worktree the ledger drifts from many owners at once, so **no single
  subagent can reconcile it — the main agent must, after every subagent has landed.**
  A subagent forbidden to edit the baseline must REPORT its new number instead.
- A pasted mismatch list goes stale within minutes. Re-run the gate yourself as the
  source of truth rather than trusting even a recent list.

**Before fanning out:** `rg <target-path> loom/maintainability-baseline.txt` for every
file the work will touch, and design the extraction into an unledgered sibling
(`signals/helpers.rs`, `format/helpers.rs`, `cli/types_ops.rs`) into the brief up
front.

**Current pressure to know about:** fifteen files sit in the 390-400 band, i.e. one
edit from tripping the gate, and two are exactly AT 400 —
`orchestrator/signals/tests_doctrine.rs` and `commands/clean/sessions.rs`. Others
include `utils.rs` 399, `terminal/native/detection.rs` 399, `verify/before_after.rs`
398, `terminal/tmux/reconcile.rs` 398, `terminal/native/mod.rs` 398,
`context/refresh/source_graph.rs` 397, `git/merge/in_progress.rs` 396,
`context/coverage.rs` 394, `signals/format/brief.rs` 392, `verify/wiring_detection.rs`
390. None is a violation today. `wc -l` your target first; at >=390 plan the split in
the same round, because a split then collides with the wiring pins below.

**The scanner walks fixtures too.** `tests/maintainability/scanner.rs` parses EVERY
`.rs` file under the crate including `tests/fixtures/`, and returns `Err` on unbalanced
braces — so a deliberately-unparseable fixture fails the gate with an error pointing at
the fixture, not at real code. Fix used: name it `syntax_error.rs.broken` and have the
test pass a virtual `.rs` dispatch path to the extractor. **Any intentionally-invalid
fixture must not carry a real `.rs` extension.**

## Goal-Backward Wiring Checks Pin a PATTERN to a PATH

A stage's `artifacts` and `wiring` lists in `.work/stages/<id>.md` name exact file
paths and exact literal patterns (e.g. pattern `extract::SourceGraphExtractor` in
source `loom/src/context/refresh.rs`). Any later change that moves the pattern out of
that path reports a wiring gap **for a feature that works perfectly**.

Hit twice in one plan, in two different shapes:

1. A file split for the 400-line limit deleted the pinned path.
2. Extracting two field assignments into a well-named helper removed the literal from
   the pinned file. Behaviour unchanged, every test green.

**The misleading signal is that the refactor is genuinely BETTER code** — a helper that
sets two related fields together reads well and keeps a ledgered file small — so
nothing in the diff looks wrong.

**Rules:**

- Treat each pattern+path pair as a **pinned interface**. `rg` them after every
  refactor round, not only at the start.
- When splitting a file, use the edition-2021 layout `<name>.rs` + `<name>/` subdir
  (as `context/graph_store.rs` + `graph_store/` already do) — **never
  `<name>/mod.rs`**, which deletes the pinned path.
- When the honest fix conflicts with a line-count ceiling on the pinned file, recover
  the line inside that file rather than leaving the pattern hidden. Keeping the pinned
  literal at the pinned call site (inlining the assignment) beats delegating it.
- **For plan authors:** pin wiring patterns to the symbol's DEFINING module, where it
  cannot migrate, not to a call site a future extraction will relocate.

**A third shape: a LATER stage can break an EARLIER, already-merged stage's pin.**
Extracting `take_down_stage_agents` out of `orchestrator/core/event_handler.rs` into
`event_handler/stage_takedown.rs` (done purely to stay under the 400-line file limit) broke
an upstream stage's own wiring check, which pinned the literal pattern `kill_session` to
`loom/src/orchestrator/core/event_handler.rs` — and the stage-completion command refuses on
an AGGREGATED wiring re-verification that re-checks EVERY already-merged stage in the plan,
not just the one currently finishing, LONG AFTER the pinning stage had already merged. The
error names the pinning stage, which reads like that earlier stage regressed, when it is the
CURRENT stage's refactor that moved the code — read the file the pattern names, not the stage
the error names. Two rules follow: before extracting code out of any file, `rg` every stage's
wiring patterns for that file's path, since a refactor that satisfies one gate
(maintainability) can silently break a different, already-closed gate (wiring) belonging to a
sibling stage; and when the honest fix is impossible without moving the code, satisfy the
pattern honestly (e.g. a comment at the new call site naming the moved-to file) rather than
moving code back just to appease a grep.

## And While You Are Reading Plan Pins: Two Wiring-Test Traps

Both blocked a stage after all its acceptance criteria had passed.

- **A `wiring_test` must never reference the plan file that is executing it.** Loom
  renames the active plan to `IN_PROGRESS-<name>` in the MAIN repo at run start, and
  the worktree was cut from the base branch, so the named plan is not on the branch at
  all. Point such a check at a plan committed on the base branch instead. The same
  reason makes the plan unreadable from an integration-verify worktree: a plan authors
  want readable there must be COMMITTED before `loom run`.
- **Every wiring_test path is relative to WORKTREE + working_dir, not the repo root.**
  A `cd loom &&` prefix made `doc/plans/...` resolve to `loom/doc/plans/`, which does
  not exist; plans live at repo-root `doc/plans/`, so the path needed `../`.

## Related

- `mistakes/tests-that-cannot-fail.md` — the sibling class where the pinned literal is
  an assertion rather than a path.
- `mistakes/testing-and-lint.md` — the ledger as part of the wider lint discipline.
