# Briefs And Bug Reports

> Stage bug reports; brief guard flags

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
**Recurred (2026-09-26, outside a stage):** a doctrine brief asked for a changed exit-5 message in `subagents/wait/identity.rs` (396 lines) without naming the 400-line limit; the worker added a 10-line helper and the file failed `tests/maintainability.rs` at 406. The orchestrator replaced the helper with a message-suffix const. Every brief that edits a file within 20 lines of 400 names its current length and the limit.
**Recurred (2026-09-30, outside a stage):** five bug-fix briefs named the 400/50 limits but not the ratchet. The workers grew six baselined entries by 1-7 lines (`#[serial]` attributes in `repair/tests.rs`, a longer `try_approve_review` doc, one line each in `DaemonServer::start` and `start_stage`). Before writing a brief, grep `loom/maintainability-baseline.txt` for every file the brief lets the worker touch, and list the matches with their current values.

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

## A Design Summary Offered for Agreement Separates Agreed From Proposed

A design summary the operator is asked to approve lists what the operator agreed, and apart from
it, what the agent proposes, each proposal labelled open. A source document's recommendations,
such as a report's implications list, stay proposals until the operator accepts them. When the
operator names a mechanism, the design works within it and never substitutes another. The
operator's wording is kept exactly: a summary that paraphrases a decision can change it.

## Brief Markdown Must Survive the Pre-Commit markdownlint Fix

The pre-commit hook runs markdownlint with automatic fixes on every staged markdown file and re-stages the result. Two constructs come out damaged: a `##` heading wrapped onto a second line (the fix inserts a blank line, leaving the second half as a stray paragraph), and backslash-escaped backticks inside an inline code span (Markdown has no such escape, so the span boundaries shift and the fix deletes the spaces between later spans). Keep every heading on one line, write a code span that contains backticks with double-backtick delimiters, and escape `|` as `\|` inside table cells. Run `bunx markdownlint-cli2 <files>` before committing briefs and read what it would change.

## A Hook Diagnosis Read From the Enforce Path Alone Missed the Early Exit

**What happened:** Asked whether loom blocks the official Codex plugin outside a stage, the orchestrator read `enforce_forwarder` (`codex-forward-guard.sh:312-337`) and a test pinning an outside-stage block, told the user the plugin was blocked, and briefed an opus subagent with a settled decision to add a stage gate. `block_forwarder` already calls `require_stage_evidence`, which exits 0 without stage evidence, so the plugin was never blocked in an ordinary session; the pinned test supplies partial stage evidence. The subagent saw `require_stage_evidence` and implemented the brief anyway, treating the settled decision as binding over the code.
**Why:** The diagnosis traced from the blocking call backwards and never read the function every block passes through; `loom knowledge context` was not queried for the hook, and `architecture/codex-plugin.md` documents the gate. The brief framed a wrong premise as settled, which discouraged the subagent from pushing back.
**Prevention:** Before stating that a hook blocks something, trace from the hook's entry to the block, reading every early exit on the way (the block helper's own body included), and pull `loom knowledge context --query "<hook> <behaviour>"`. A brief built on a hook diagnosis quotes the traced path. A subagent whose tree contradicts its brief's stated problem reports that before editing.
**Fix:** The subagent was redirected, reverted every edit, and ran each registered hook with a no-stage payload: none blocks, rewrites, or records a `codex:codex-rescue` call.

## A Stage Bug Report's Suspected Cause Is a Lead, Not a Diagnosis

**What happened:** the stage's bug report (BUGS-map-api-freshness-session.md, removed once its entries were fixed) blamed a stuck integrity dispute on the dispute apply path or a lost update. The session started planning fixes from those suspicions, and the user stopped it: validate first. The orchestrator log and shell history showed a different chain. `loom update` had replaced the binary under the running daemon, so the adjudicator spawn failed. The stage was escalated to NeedsHumanReview with its disputing worker still alive. An operator `--approve` then re-queued the stage, and adoption re-attached the idle worker.

**Why:** a stage session writes its report from inside a sandbox. It cannot see `orchestrator.log` timing, operator commands, or daemon restarts, so its suspected causes are guesses about code it never traced.

**Prevention:** before fixing anything from a stage bug report, confirm each entry against the tree and `.loom/work/orchestrator.log` (grep the stage id around the reported timestamps). Record a verdict and root cause (file:line) per entry. Only then decide scope with the user.

**Fix:** the entries were validated one by one, and their fixes were scoped from the confirmed causes.

## Described an In-Flight Edit as Installed Behavior

**What happened:** while a delegated engineer was fixing how `loom stage retry --context` reaches the retried session, the main agent read `commands/stage/recover.rs` from the working tree, found `with_operator_answer`, and told the operator that loom already delivers the `--context` text verbatim. That function was the engineer's uncommitted change. The installed binary only marked the retry as a manual recovery and dropped the text, so two operator answers were lost and the stage blocked twice more.

**Why:** the working tree was not the code the operator runs; a subagent was editing it at the time.

**Prevention:** before telling an operator how loom behaves today, read the code at `HEAD` (`git show HEAD:<path>`) or test the installed `loom`, never the working tree while a subagent writes to it.

**Fix:** the operator was told the text had been dropped and given the commands that work with the installed binary.

## A Bug Report's Proposed Remedies Were Adopted Without Checking Them Against the Tree

**What happened:** an incident write-up proposed a new `loom stage contracts run` command and told the contract writer to set `cwd` on its contracts. Both went into a scope proposal unchecked. The freeze already runs and prints every contract's command, and the contract writer cannot edit plan metadata.

**Why:** the write-up's remedies were read as requirements, not as hypotheses about a failure chain.

**Prevention:** for each remedy in a bug report, find the code that already serves it and the actor who would carry it out before adopting it. Keep only remedies that close a gap the tree has.

**Fix:** the scope was rebuilt from the failure chain: run-dir resolution, freeze acceptance of unreadable reds, plan verify, dispute granularity, stale `contracts show`.

## Match each audit claim to its cited symbol

**What happened:** The audit draft attached the settings response replacement claim to settings-model.ts, which actually defines pending-key helpers; replaceEntry lives in settings-dialog.tsx.

**Why:** Adjacent source citations were transposed while condensing reviewer notes.

**Prevention:** Validate the symbol-to-claim association as well as file existence and line bounds before finalizing an audit.

**Fix:** Cite settings-model.ts for statusKey and settings-dialog.tsx for replaceEntry; keep the independently confirmed UI race finding.
