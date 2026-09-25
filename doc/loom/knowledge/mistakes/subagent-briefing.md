# Subagent Briefing

> Briefs, wave sizing, file ownership

## Never Hand Over a Proving Command You Have Not Run

`cargo test --lib a:: b:: c::` was written into a subagent brief. **Cargo accepts exactly
ONE testname filter** and rejects the extras with "unexpected argument" BEFORE compiling,
so zero tests ran. The error mentions the argument, not the arity, so it reads like a typo
in a test path. This bit both the orchestrator and a subagent in the same session.

**Rule:** any proving command handed to a subagent must be one you have actually run
yourself — a subagent that cannot run it reports "no tests ran" and you learn nothing
about its slice. Use ONE common prefix (`cargo test --lib context::`) or separate
invocations chained with `&&`.

Related: test module paths are part of the filter. The delivery unit tests live at
`context::tests::delivery`, so `cargo test --lib -- context::delivery` matches ZERO tests.

## Verify a Delegation Before Rejecting It

The installed `codex-forward.sh` did not emit the evidence trailer the codex doctrine says
to require, and treating that absence as proof the wrapper self-implemented would have been
wrong. Confirm a forward two other ways: the stdout carries the codex thread transcript
(`[codex] Starting Codex task thread.`, `[codex] Applying N file change(s).`), and a job
record exists under `~/.claude/plugins/data/codex-openai-codex/state/<worktree>-<hash>/jobs/*.json`
with a recent mtime. A forwarder that refuses to invent a trailer it never saw is behaving
correctly.

**And read what codex touched.** A codex forward that edits a small list-style file can
mangle it: `loom/tests/integration/mod.rs` came back with four lines spuriously indented
and the new entry out of alphabetical order. `cargo fmt` does NOT fix a mod list's
indentation or ordering, so it survives the format gate and only shows up in review.
"Check what it touched" means READ the diff of any small structural file (`mod.rs`,
`Cargo.toml`, a registration list) — whitespace damage passes every automated gate.

## Apply the Cheapest-Capable Rule to UNPLANNED Spawns Too

Mid-stage integration tasks (test-expectation fixes, a module split) were defaulted to
sonnet without deliberation on a stage that licensed codex, because the plan's EXECUTION
PLAN named codex for the PLANNED workers and lane choice felt already decided. Unplanned
work silently inherited the session default instead of getting its own lane decision. The
user called it out.

**Test to apply per spawn:** if you can name the exact files, the exact target shape, and
every signature that must survive, it is codex terra work. Route to sonnet when the work
needs open-ended judgment — not because codex cannot look things up.

**Superseded claim, corrected:** this section used to say "codex has no Read tool and
pages files through the shell in ~160-line chunks (measured 9m45s unscoped versus 54s
scoped)," and concluded that open-ended repo exploration must go to sonnet for that
reason. That measurement came from codex being told to read CLAUDE.md and sweep
`doc/loom/knowledge/` wholesale — not from exploration being inherently slow for codex.
Loom's forwarding wrapper now hands every codex prompt a source-graph navigation kit
(`loom map --find-all` / `--outline` / `--impact`, `loom knowledge context --query`), so
codex navigates by querying loom's index instead of paging files
(`architecture/codex-plugin.md#the-navigation-kit`). Codex is no longer excluded from
exploration on that basis; the cheapest-capable test above still governs the lane choice.

## Never Add Work to a Subagent by Message — a Queued Follow-Up Double-Assigns Files (2026-09-12)

**What happened:** a retry-test task was sent by `SendMessage` to a `fix-tests` worker
while it was already finishing. The cancel that followed raced it: a fresh
`fix-tests-2` was spawned for the same test files, but the first agent's queued message
resumed it anyway, and it wrote `settings-entry.test.tsx` while `fix-tests-2` owned that
file — the two-writer collision Rule 6 exists to prevent.

**Why:** a queued message resumes an agent even after it has already reported; a
`TaskStop`/cancel sent around the same time does not reliably beat that resumption.

**Prevention:** follow Rule 6 of the loom-orchestration skill to the letter — never add work to a subagent by
message. A follow-up, or a continuation after a report, is always a FRESH spawn briefed
with the prior report; if a message was already sent to an agent still `tool-wait`/
`generating`, wait for that agent's reply before assigning its files to anyone else.

## A Worker's Self-Reported Size Compliance Is Unverified Without a Measuring Command (2026-09-12)

**What happened:** a worker tasked with splitting oversized functions reported every
result "well under 50 lines, verified by re-reading each" — a TypeScript-AST line count
run afterward found 8 functions still at 51-83 lines. Separately, `oxlint -c <cfg>` with
`max-lines-per-function` printed nothing even against a known 112-line function, so it
could not have served as a check either.

**Why:** an agent's own re-read is an estimate, not a measurement, and a linter rule
that has never been proven to fire on a known violation is not a working check — both
failures look identical from the outside ("checked, clean") until an independent
measurement runs.

**Prevention:** give any size-limit brief a concrete measuring command as its acceptance
check — a small standalone script built on `ts.createSourceFile` + `ts.isFunctionLike`,
flagging any span with `end - start + 1 > 50` — and before trusting a clean result from
any checker, prove it fires on a file with a known violation.

## A Stage Ran 14 Subagents Against a 4-Worker Budget — Unverified Rework, Not Scope (2026-09-12)

**What happened:** the settings-lanes stage budgeted 4 sonnet workers; the orchestrator
ran 14, and the user called the stage "way too long for a simple task." About half the
wall-clock was rework the orchestrator could have prevented before it happened, not
extra scope the task genuinely needed.

**Why, concretely:**

- File sizes were checked once instead of after every report — a 441-line test file
  (over the 400-line limit) slipped through and grew to 504 lines before the next round
  caught it.
- A size-limit brief carried no measuring command, so a worker's estimate of function
  length was simply wrong (see the entry above).
- A follow-up sent by message to a finishing worker caused a double file assignment
  (see the entry above).
- A sandbox limit (loopback TCP denied even though the dev server reported listening —
  [sandbox-tooling-and-network.md](sandbox-tooling-and-network.md#a-stage-sandbox-can-deny-loopback-tcp-even-while-the-server-reports-listening-but-not-always-2026-09-12))
  was discovered only after several failed dev-server attempts, well into the stage.

**Prevention:** run `wc -l`/the measuring command over every file a worker touched
before accepting its report, every time, not just on the first wave; put the command in
the brief instead of trusting an estimate; never add work to a subagent by message
(spawn fresh instead); and probe sandbox limits the plan depends on (loopback, browser)
while wave 1 is still running, not after a later wave needs them and fails.

## A Brief's Derived Value Needs Its Type Stated, Not Just Its Shape (2026-09-12)

**What happened:** a foundation brief's `LaneSlot` contract computed a display variable
as `"error" | "pending" | state.provenance` in prose, but the plan's own type for
`state.provenance` was `Provenance | "readonly"` — the brief never gave the COMBINED
value a type. The worker's literal transcription of the prose failed `tsc` (TS2322),
and the worker could not typecheck its own work in isolation because a file it
depended on still imported a symbol (`railStops`) from the provenance-rail component the
plan was mid-way through deleting.

**Prevention:** when a brief quotes a derived value built from more than one field,
state its TypeScript (or equivalent) type explicitly, not just the expression that
produces it. When a foundation worker cannot typecheck cleanly because of an
in-progress deletion elsewhere, still have it run the type-checker scoped to its own
files (`tsc --noEmit -p` with an explicit `include`) rather than skipping the check
entirely.

## A Brief's Worked Example Must Be Checked Against the Real Fixture, Not Trusted (2026-09-12)

**What happened:** a brief's worked example claimed a derived boolean
(`projectAllowed`) came out true only for two of several sections in a fixture file.
The actual fixture (`web/src/api/fixtures/config.json`) gives every `pressure.*`/
`models.*` entry a project scope too, not just the two the brief named — the
IMPLEMENTATION rule the brief stated (`projectAllowed = any entry.scopes includes
project`) was correct, but its worked numbers, copied into a test's expectations,
would have been wrong.

**Prevention:** before writing test expectations from a brief's worked example, open
the actual fixture file the code will run against and recompute the example by hand;
do not transcribe a brief's illustrative numbers as ground truth.

## A Test-Rewrite Brief That Lists Cases Can Silently Drop an Unlisted describe Block (2026-09-12)

**What happened:** a brief for rewriting `settings-dialog.test.tsx` listed 12 target
test cases. The rewrite dropped the file's "header entry point" block — real routes,
fetch stub, and a header click — the only test exercising header-to-dialog wiring,
because the brief never named it and the block was not one of the 12 cases replaced.

**Why:** a brief that enumerates NEW cases says nothing about which OLD cases must
survive; a worker rewriting the file has no signal that an unlisted block was load-
bearing coverage rather than an artifact of the old implementation.

**Prevention:** a test-rewrite brief must say explicitly "keep every existing
`describe`/`it` block not replaced by a listed case." Before accepting a test-rewrite
report, diff the file's block names old vs new
(`git show HEAD:<file> | rg '^\s*(describe|it)\('`) rather than trusting that the
listed cases were the only ones that mattered.

## A Brief With 5-6 Tasks Across a Dozen Files Overruns the Harness's 150-Turn Limit (2026-09-13)

**What happened:** in the state-confinement plan, phase-1 worker I1 (sonnet) and phase-3 workers B1
(sonnet, 170 tool uses) and D (sonnet, 164 tool uses) all stopped mid-edit at the harness's 150-turn
limit WITHOUT a report, leaving the tree in an unknown, sometimes non-compiling state (B1 had
deleted a function a test still called).

**Why:** each brief carried 5-6 tasks spread across roughly a dozen files — too much for one sonnet
run to finish and still leave a report.

**Prevention:** keep a brief to at most ~3 tasks or ~6 files. Ask the worker to keep a report
skeleton (files changed so far) current throughout the run, so a cut-off run still leaves something
readable. Always inspect the actual tree state before trusting a silent stop — no report does not
mean no changes.

## A Coordinator Wave for a Mid-Sized Feature Cost Forty Minutes (2026-09-12)

**What happened:** a configuration feature touching 35 files (fourteen config keys, two project sections, pressure flags, launch and dashboard wiring, docs) was run as one opus coordinator over five sonnet workers in two waves. Wall clock from spawn to a verified tree was about forty minutes; the user called that unacceptable for the size of the work. The coordinator itself spent turns briefing, the second wave could not start until the first returned, and the main agent still had to fix two seams the workers left (a key-level fallback that returned the built-in instead of the user tier, and an integration test that pinned the old dev-build notice).

**Why:** the two-level shape trades wall clock for context isolation. It pays off when the territory is too wide for one brief, not when the work is a handful of well-mapped files per lane. Here every lane was already mapped in the main agent's brief, so the coordinator added a hop without adding judgment.

**Prevention:** when the main agent has already mapped every file and signature, spawn the lanes directly as a flat fan-out and skip the coordinator, even if the user suggested one; say so and proceed. Sequence only what truly depends on a foundation, and give dependent workers the foundation's signatures up front so they start at once. Budget wall clock explicitly: a lane that needs more than fifteen minutes is a lane whose brief was too vague.

**Fix:** none in code. The feature shipped; the lesson is about shape.

## Large-Scale Parallel Doc Editing: Whole-File Writes Fail, Self-Lint Reports Lie (2026-07-01)

**What happened:** During the 61-file skills/ overhaul (4 coordinators × ~6 workers), two recurring failures: (1) workers rewriting very large files (~2-3K lines, e.g. `skills/loom-react/SKILL.md`) with a single whole-file Write died repeatedly with "Connection closed mid-response" (0 tokens, files untouched); the same file succeeded when the worker was re-instructed to apply ~18 small targeted Edits instead. (2) Workers self-reported "markdownlint clean" but a single authoritative `markdownlint-cli2` pass at the gate found 35 residual errors across territories (MD032/MD056/MD038/MD034/MD028) — worker self-verification via ad-hoc greps does not implement markdownlint rules.
**Why:** A multi-thousand-line Write is one giant model response — long uninterrupted output maximizes exposure to connection drops, and a failure loses ALL of the work; incremental Edits checkpoint progress per tool call. Lint self-reports were grep-approximations, not the real linter (bunx was sandbox-blocked for workers: bun needs tempdir writes outside the sandbox allowlist).
**Prevention:** (1) When directing agents to rewrite files >~1000 lines, instruct them to transform via a sequence of targeted Edits, never one whole-file Write. (2) Never trust per-agent lint claims — run ONE authoritative `bunx markdownlint-cli2 "skills/**/SKILL.md"` at the merge/verify gate (no sandbox escape needed — point `TMPDIR` and `BUN_INSTALL_CACHE_DIR` at a writable scratch dir; recipe in `mistakes/testing-and-lint.md`); the repo `.markdownlint.json` is picked up from the root.
**Fix:** Re-spawned failed workers with the incremental-Edit instruction; ran the gate lint pass and fixed the 12 residual errors (main agent) + 23 (backend coordinator) directly.

## Messaging a Stopped Subagent Resumes It Instead of Reaching a Fresh One (2026-09-13)

**What happened:** a continuation message was sent to worker B1 believing it was still running; it
had already stopped at its turn limit (the previous entry). `SendMessage` resumed it rather than
failing, so the orchestrator's model of ownership — B1 still owns its files — was wrong with no
error surfacing.

**Why:** `SendMessage` does not distinguish "agent paused mid-turn" from "agent's turn ended"; both
accept a new message and continue.

**Prevention:** check the agent's actual state (`loom subagents list --session ...`) before sending
it a message. A stopped agent's files are free for the orchestrator to route to a fresh spawn — do
not message it expecting a continuation.

## File Exclusivity Is a Property of ALL Live Agents, Not of One Wave

A second subagent was spawned whose file set overlapped a still-running one: the
orchestrator asked a fixer for one extra test file, then — before it reported — spawned a
refactorer whose brief also covered `commands/knowledge/context.rs`. The fixer was wiring
a test module into that file while the refactorer extracted a helper out of it; whichever
wrote last would silently drop the other edit.

**The misleading signal:** the fixer LOOKED finished, because its seven earlier fixes had
been verified on disk and its earlier report had arrived — but it had just been handed
more work, so it was live again.

**Prevention:** before spawning any agent, list every agent that has not reported SINCE
ITS LATEST assignment and diff the file sets. **Asking a finished agent for one more thing
makes it live again and re-arms the conflict.** Detection used: grep the specific wiring
line the earlier agent added, both before AND after the later agent finishes.

## Three of Four Review Findings Were Specification Gaps, Not Implementation Errors (2026-09-21)

**What happened:** an adversarial review of a finished TUI feature returned four defects. Only one
was the implementer departing from its brief. The other three traced back to the brief itself:

- The brief said the inspector strip must drop segments "from the right" and "never drop the
  `s → path` segment" — two rules that contradict each other, because that segment IS the
  rightmost. The implementer built it last from leftover width, making it the only droppable one.
- The brief said to dim a row that is "`Unbacked` under Project scope". Two distinct states make a
  row untouchable (`Unbacked`, and `NoWorkspace`); naming one variant produced a screen that dims
  for one reason and not the other.
- The brief specified a quit guard as "press q again" without saying which keys count as the
  confirming second press. `q` and `Esc` were already aliased at the dispatch site, so `Esc` —
  which cancels an edit three lines away in the same file — became a confirm.

**Why:** each gap has the same shape. A rule was written as prose about the COMMON case and left
the boundary unstated: which element is exempt when the exemption is positional, which states the
predicate covers when more than one qualifies, which inputs a two-step confirmation accepts. An
implementer resolves an unstated boundary the cheapest way, and the cheapest way is usually the
literal reading.

**Prevention:** when a brief states a rule with an exception, an ordering, or a confirmation step,
write the boundary as well as the rule:

1. **An exemption that is positional contradicts an ordering.** "Drop from the right, but never drop
   X" needs to say X is reserved FIRST and the others laid out in what remains.
2. **Name the predicate, not one variant.** "Dim a row the active scope cannot touch, derived from
   whether it has a value here" survives a third variant; "dim `Unbacked`" does not.
3. **A confirmation step names its accepted keys and its cancel key.** Otherwise whatever alias
   already exists at the dispatch site inherits the meaning.

Also worth keeping: the defect the review ranked worst existed BECAUSE its test could not fail for
it — the quit-guard test only ever pressed genuine non-quit keys as the second key. A brief that
adds a guard should require the test to be shown failing against the old behaviour, which is the
only thing that proves it reaches the path. See [[tests-that-cannot-fail]].
