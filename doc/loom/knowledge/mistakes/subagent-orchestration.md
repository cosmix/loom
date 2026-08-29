# Subagent Orchestration

> Topic notes for the mistakes knowledge area.

## A Missing Report Is Not a Missing Result

**The single most expensive orchestration belief in this repo.**

In one source-graph session, five worker subagents and two `loom-code-reviewer`
subagents all EDITED FILES CORRECTLY and **never delivered a final report** — no task
notification arrived for over 90 minutes for the first batch. One worker demonstrably
acted on a `SendMessage` (it fixed the exact function named, 9 seconds before the next
`stat`) yet still sent no reply. The knowledge-distillation stage of the same plan then
spawned six read-only gatherers and received zero reports from any of them.

**Prevention:** do not treat a missing report as a missing result. **Verify the WORK
directly** — run the gate, `stat` the files, read the diff — and absorb the outcome from
the tree. An orchestrator that waits on reports alone will hang forever here: budget one
real blocking wait, then verify and move on.

**Corollary for read-only agents.** A gatherer whose entire deliverable IS its report
has no work to verify, so this failure mode destroys its output entirely. When the task
is "read this and summarise", prefer doing it yourself, or have the agent write its
findings to a file inside the worktree that you can read regardless of whether it
reports.

## Liveness Is mtime, Never Size

File LINE COUNTS are a **false negative** for liveness: a worker rewriting a function in
place holds `wc -l` steady for many minutes while working hard, which reads exactly like
a dead agent. One session almost took over five live workers' files after roughly ten
consecutive stable-line-count checks; a `stat` showed the newest write had landed 9
seconds earlier.

**Rule: liveness = mtime moved, never size moved.** Use `stat -c %y <files>` or
`fd --changed-within 5min`. Restarting live work forfeits every token it has spent and
sets two agents writing the same files.

**Superseded as the PRIMARY signal (2026-08-27), not deleted.** `loom subagents
list`/`watch` read each subagent's own transcript and report `done`, `tool-wait`,
`generating`, or `unknown` — a distinction mtime cannot make, since a file sitting still
is consistent with both "finished" and "deep in one long tool call". Prefer `loom
subagents` first; mtime stays true as a fallback and as the reason NOT to trust file
size.

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

## "I Wrote My Own Because I Could Not Reach Yours" Is a Defect Report

When a worker says it reimplemented something rather than calling it, that is a
duplication defect to fix at integration — **never a resolved decision**. In one round it
produced a second copy of a security-critical untrusted-content fence renderer, which is
exactly the rule that must not drift between surfaces. See
`mistakes/visibility-and-reachability.md` for why the worker could not reach the original.

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
every signature that must survive, it is codex terra work. If the task needs open-ended
repo exploration, it is sonnet — codex has no Read tool and pages files through the shell
in ~160-line chunks (measured 9m45s unscoped versus 54s scoped).

## Related

- `mistakes/verification-harness.md` — when every check fails at once, suspect the harness.
- `patterns/subagent-hierarchy.md` — choosing flat fan-out, a coordinator hierarchy, or a team.

## Before Its First Write, an Agent Had NO Liveness Signal At All (2026-08-17; CLOSED 2026-08-27)

The mtime rule above assumes the agent has already written something. Between spawn and
first write there used to be **no negative evidence available** — not an empty `git
status`, not an absent file, not `ListAgents`. An interactive session re-dispatched a
second agent onto a live one's file set on exactly that reasoning ("no changes on disk,
and `ListAgents` reports nothing reachable, so it must be dead"). It was
mid-investigation and had simply not typed yet. The two agents then wrote competing test
layouts, a third was sent in on a stale snapshot, and at the worst moment one was
deleting another's files while the suite sat at 5 red. Roughly 20 minutes and three
agents of work were burned; the production fixes had been correct the whole time.

**`ListAgents` returning "No reachable agents" is NOT evidence of death.** It went on
returning that while three spawned agents were actively editing files, and it said it
about agents that had already delivered final reports minutes earlier. Treat it as
"cannot tell", never as "gone".

**The blind spot is CLOSED: Claude Code writes every subagent its own JSONL transcript
from its FIRST TURN, before it writes any file.** It lives at
`~/.claude/projects/<project-slug>/<session-uuid>/subagents/agent-<agentId>.jsonl`
(`<project-slug>` is the absolute cwd with every `/` and `.` replaced by `-`), one JSON
entry per line carrying `agentId`, `timestamp`, `type` (`assistant`/`user`), and
`message.content`. `loom subagents list` reads this, so a spawned agent has a liveness
signal immediately — the incident above could not recur today.

**Prevention: run `loom subagents list` (or `watch --timeout <secs>`), not "wait because
you have no signal".** Per-subagent state is `done`, `tool-wait`, `generating`, or
`unknown`: `done` but silent means harvest from disk and proceed — a missing notification
is not a missing result (see "A Missing Report Is Not a Missing Result"); `tool-wait` /
`generating` means genuinely alive, re-arm and keep waiting; only idle time past the
budget with NO transcript growth is positive evidence of death. Two measured numbers set
that budget, not estimates: a single tool call was clocked at 1,425s (23.8 minutes) in
one real transcript, which is why `tool-wait` must NEVER carry its own timeout no matter
how long it runs; and true intra-turn flush gaps (pauses between transcript writes within
one still-live turn) topped out at 137.7s across 8,808 sampled gaps, which is why a
`done` classification is debounced by 180s rather than trusted the instant output stops.
If a takeover ever does look necessary, `TaskStop` the original FIRST, confirm it
stopped, and only then dispatch a replacement — never leave two writers pointed at one
file set. Recovery from a collision is the same discipline: hard-stop every agent, take
one snapshot of the frozen tree, decide the target layout yourself, then send exactly one
agent to converge it.

**Corollary — a stale brief is worse than no brief.** Each replacement was briefed from a
snapshot that had already moved, so it was told to create files that already existed and
to fix problems already fixed. When dispatching into a tree others have touched, re-read
the state immediately before writing the brief, and tell the agent to STOP and report if
what it finds contradicts the brief. The one agent that did exactly that is the only one
that cost nothing.

## An Idle Notice Is Not a Report, and Absence of an Edit Is Not Refusal (2026-08-27)

**What happened:** in one interactive fan-out, two subagents were sent follow-up corrections via
their mailbox. Both emitted `idle_notification … "available"` shortly after. Reading the files
showed neither correction applied, so both were stopped and the remainders re-delegated to fresh
agents. The fresh agents then reported that the work was **already there** — the originals had
been mid-edit and finished in the window between the check and the stop. One re-delegation was
genuinely needed (that agent had written only the test, not the fix); the other was pure waste.

**Why:** `idle` means "not currently in a turn", not "processed your message". And a file read is
a point sample: an agent that has read the brief and is composing an edit looks byte-identical to
one that ignored it. Combining the two — idle notice plus no visible edit — feels like conclusive
evidence of non-compliance and is not.

**Prevention:**

- **Only a completion report closes a delegation.** Stop an agent on its own report, or on the
  liveness rules above (transcript not growing past a real budget) — never on "went idle and I
  don't see the change yet".
- **Verify against the tree, then wait, then verify again** before concluding an agent is not
  acting. The second sample is what distinguishes mid-edit from ignored.
- **What DID pay off: reading diffs instead of reports.** Two agents reported work as complete and
  clean that was neither — one had a path-traversal bypass in a security guard
  (`mistakes/untrusted-value-boundaries.md`), another left a status arm that rendered a stalled
  loop as healthy. Both reports were detailed and confident. Review the diff of anything
  load-bearing; a subagent's summary is a claim, not evidence.
- **A ledger or gate the agents cannot see will bite at commit time.** Five files broke the
  repo-wide line-count gate because every agent added regression tests and comments in good faith.
  Check the gate BEFORE fanning out, or expect a second round purely to satisfy it.
