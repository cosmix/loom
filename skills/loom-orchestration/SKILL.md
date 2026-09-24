---
name: loom-orchestration
description: Load before spawning subagents or running a loom stage as its main agent; skip for single-agent work. Delegation cost, briefs, waits, commits.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
  - Task
triggers:
  - spawn subagents
  - spawning subagents
  - delegate implementation
  - subagent brief
  - worker brief
  - fan out
  - file ownership table
  - coordinator preamble
  - worker preamble
  - subagents watch
  - harvest subagents
  - orchestrator commits
  - escalate tier
  - two-level hierarchy
---

# Loom Orchestration

The rules a session needs when it spawns subagents or runs a loom stage as its main agent.
`~/.claude/CLAUDE.md` keeps a short statement of each; the full text is here, filed under the
CLAUDE.md rule number it belongs to. A single-agent session does not need this skill.

## Session start

Read your Knowledge Brief, find your session ID in `.loom/work/signals/`, scan for `EXECUTION PLAN`
blocks, and spawn per Rule 6; with no signal, ask the user.

## Rule 1 — Plans and stage execution

**Loom plans:** create one only on explicit request for loom orchestration ("use loom", "loom
run", "create a loom plan") or `/loom-plan-writer`, always authored via `/loom-plan-writer`,
canonical for every plan mechanic via `loom plan verify`. Never write loom YAML from memory.

**Executing a stage:** your signal file carries the stage's tasks and completion checklist (memory
recording, wiring verification, mini adversarial code review, commit steps); it is binding, and you
work it to completion.

## Rule 4 — Commit and complete

The commit is legitimate only once all three hold: (1) every subagent, coordinator, team, and
Workflow returned and was absorbed; (2) the full gate (build, tests, lint, format, acceptance) is
green on the complete tree; (3) the mini adversarial code review returned, every finding fixed,
gate green again. In a plan version 2 stage `loom stage complete` enforces condition (3): it fails
until a recorded review round matches the worktree and no finding is open. A handoff is no reason
to commit unverified work — record uncommitted files there instead.

**Complete ONLY a settled stage; completion is the session's LAST act.** All three conditions still
hold (an abandoned subagent counts as returned only if you recorded why), every defect fixed and
re-verified, `git status` clean. The stop hook only WARNS; confirming completion succeeded is your
job. Afterwards STOP: post-completion work is LOST WORK.

Group commits logically: 5 files across 3 concerns = 3 commits, not 1 or 5.

## Rule 5 — Subagent preamble

Subagents write code and report; the main agent owns verification and all git operations (hard
stop 3). Every typed Claude spawn opens with the preamble in `loom-hooks/_subagent-preamble.txt`
(installed beside `spawn-guard.sh`). The spawn guard prepends that file to any typed spawn whose
prompt lacks its first line, so you do not paste it; a prompt that already opens with that line,
such as a coordinator's (below), is left alone. The file carries the git restrictions, the
no-verify rule (BLOCK-A), the context-ceiling rule (BLOCK-D), memory recording, and the
auto-memory prohibition.

Claude subagents only; `loom-hooks/codex-forward.sh` prepends codex's own rules to forwarded
prompts (Rule 7).

## Rule 6 — Subagents

| Shape | When | Cost | Who commits | Which preamble |
| --- | --- | --- | --- | --- |
| **Flat fan-out** (default) | Independent file changes, separate implementation areas, tests alongside implementation, exploration; ≤~6 worker tasks | 1 spawn level | Main agent | Rule 5 |
| **2-level hierarchy** | >~6 tasks in 2-4 DISJOINT territories, 2+ tasks each, well-defined for sonnet workers; NOT for ≤~6 tasks, shared files, cross-territory iteration, or sequential work | +1 coordinator per territory | Main agent | Coordinator preamble to coordinators; Rule 5 + worker addendum |
| **Agent teams** | Wide, exploratory work needing inter-agent messaging or dynamic task discovery; NOT for concrete assignments with clear file ownership | ~7x whole-job | Team lead, who alone runs `loom stage complete` and `loom memory` | Rule 5 |

Team lead: TeamCreate → TaskCreate → spawn → monitor (TaskList) → shutdown, verify, complete;
delegate, keep context <40%, shut down ALL teammates.

**GROUP TASKS: small tasks go to ONE subagent, never one per task or file.** Every extra subagent
costs twice — the orienting tokens spent before it writes anything, and the disjoint file set it
forces. Four files with a one-line edit each is still ONE subagent. Split only when a territory is a
separate job, or one assignment would exceed a subagent's context. The ≤~6 threshold counts
subagents after grouping, never raw tasks.

Two subagents writing one file = lost work; if unavoidable, serialize in one subagent or split
stages. Every prompt carries a file-ownership table:

| Subagent | Files Owned (write) | Files Read-Only | Shared context — quoted in the brief, do not open |
| --- | --- | --- | --- |
| Subagent 1 — [role] | `path/to/files` | `shared/config` | `patterns.md#<heading>` |
| Subagent 2 — [role] | `other/files` | `shared/config` | `patterns.md#<heading>` |

**Declared skills.** A stage may list `skills: [<skill-name>, ...]` (full names, such as
`loom-rust`; `loom plan verify` rejects an unknown name). Pass every declared skill into each
brief whose work it covers, with the invocation the stage signal names for it, so the subagent
loads it before writing code instead of rediscovering it.

**Read receipts.** Every agent of one session writes its reads ledger into one session directory:
`$LOOM_WORK_DIR/hooks/reads/<LOOM_SESSION_ID>/<agent-id>.tsv` inside a stage, a per-session
directory under `${TMPDIR:-/tmp}/loom-reads/` outside one. When a subagent reads a file of more
than 200 lines whole that a sibling already read whole and unchanged, the read guard appends
`<absolute path>\t<lines>\t<agent count>\t<UTC timestamp>` to that directory's `_shared.tsv`; the
latest row per path carries the current count. Before the next round of briefs, read it: a file
several subagents each read whole belongs in the brief as quoted `file:line` ranges.

**Checking on subagents: use one owned `loom subagents` wait, never a hand-rolled poll loop.** Spawn every worker first and capture each worker ID: the Claude agent ID from the spawn result, or the Codex unit ID you assigned with `--unit-id`. Then run ONE `loom subagents watch --worker claude:<agent-id> --worker codex:<unit-id> --timeout 3600` through the Bash tool's `run_in_background`, with one `--worker` for every worker. It binds those workers once, holds one lease for the parent session, prints one initial record and one terminal record, then exits. Treat its exit distinctly:

1. **Exit 0** — every bound worker has fresh, correlated success evidence.
2. **Exit 2** — the wait deadline passed. This is not proof that any worker died.
3. **Exit 3** — a bound worker failed or was cancelled.
4. **Exit 4** — a wait for this parent session already exists: `AlreadyWaiting` for the same worker set or `Busy` for a different set. No second monitor was started.
5. **Exit 5** — worker identity or terminal evidence is unknown. This is never success.
6. **Exit 6** — a bound worker is hung: no transcript growth past the stall budget (`--stall-secs`, default the stage's `subagent_timeout_secs`, else 600 s) for a Claude worker, or a codex job whose process is gone or whose log stopped growing. `TaskStop` the Claude worker, confirm it stopped, then RE-DELEGATE the remainder in a smaller brief.

Harvest each worker's terminal report exactly once. Never re-arm the watch and never poll with `loom subagents list`, `loom subagents harvest`, `git status`, `wc`, or `ls`; `list` and `harvest` remain one-shot diagnostics. Only exact authoritative terminal evidence permits completion. Exit 6 is the channel that reports a worker idle past the stage's `subagent_timeout_secs` budget with no transcript growth — the only positive evidence of death. `TaskStop` it, confirm it stopped, then RE-DELEGATE the remainder to a fresh subagent. Never absorb the work into yourself — the orchestrator decomposes, delegates, verifies, and commits; it does not implement (hard stop 6). Re-read the tree before writing the new brief: a stale brief is worse than no brief. Never complete the stage while any subagent is still out (Rule 4).

**Subagents are ONE-SHOT.** Brief completely, wait, harvest, let it end. Never message a finished
subagent. A follow-up, or continuation after a ceiling, is a FRESH spawn of the same type, briefed
with the prior report and remaining items. Messaging one still `tool-wait`/`generating` differs,
and is rare.

**2-level cap:** main agent → coordinators → workers. Spawn coordinators as `general-purpose` with
an explicit `model` override — the engineer agent types carry no Task tool and run as leaves, so
they cannot coordinate. Spawn workers BY AGENT TYPE — untyped ones inherit the main session model.
Coordinators delegate too (hard stop 6), going opus only when territory integration needs
judgment. Mix lanes under ONE ownership table. On a stage listing codex in `implementers`, a
coordinator spawns `loom-codex-forwarder` BY AGENT TYPE (never the plugin's `codex:codex-rescue`
directly), foreground only: gpt-5.6-terra for common implementation and integration tests,
gpt-6-luna for boilerplate, scaffolding, and simple unit tests. The lane is chosen per subagent,
never per stage; the coordinator still does not verify.

With an `EXECUTION PLAN` block, parse ALL assignments and spawn ALL in ONE message; hierarchical
plans spawn COORDINATORS, not workers. Every prompt carries the preamble, assignment, files
owned/read-only, acceptance, task detail. "Sequential" means execute in order with explicit
dependencies.

COORDINATOR PREAMBLE — first lines of every coordinator prompt. It opens with the Rule 5
preamble's first line, so the spawn guard adds nothing:

```text
CLAUDE.md is already in your context; the rules below are the ones that bind you as a subagent. The knowledge you need for this task is quoted in this brief - do not open doc/loom/knowledge/ unless the brief says a pull came back empty.

COORDINATOR ROLE - YOU ARE A SUBAGENT COORDINATING WORKERS (ONE LEVEL ONLY):
- You own ONE territory: [TERRITORY]. Never touch files outside it.
- Partition your territory into DISJOINT worker file sets - two workers writing one file = LOST WORK
- Spawn workers via the Task tool BY AGENT TYPE (loom-software-engineer = sonnet); include the WORKER PREAMBLE as the first lines of EVERY worker prompt; spawn independent workers in ONE message
- Workers NEVER spawn subagents - they are LEAVES (loom caps the tree at 2 levels)
- Delegate implementation; write at most small glue/fixes within your territory
- AT MOST ONE narrowly-scoped check over the files your workers wrote (e.g. `cargo test <your_module>::`), run ONCE; skip it if you are unsure. The MAIN AGENT compiles, tests, lints, and fixes.
- Return a COMPACT summary: files changed, verification command + result, failures/blockers, insights. No file dumps, no diffs.
- NEVER run git commit, git add -A/., or loom stage complete - only the main agent does
- Record insights via loom memory note/decision; NEVER use Claude Code auto-memory
```

WORKER PREAMBLE — the Rule 5 preamble first (the spawn guard prepends it when the prompt lacks
it), then this, with no second no-verify block:

```text
WORKER RESTRICTIONS - YOU ARE A LEAF AGENT:
- NEVER spawn subagents (Task tool) - the hierarchy is capped at 2 levels; do the work yourself
- Touch ONLY your assigned files: [FILES]
- Return a short report: what changed, anything surprising, anything unresolved
```

## Rule 7 — Model allocation

Fable-tier work has no pinned agent type; pass the model override explicitly at spawn.

1. DELEGATION IS A COST DECISION: TOKENS TIMES MODEL TIER (hard stop 6). A
   stage's main agent decomposes the work, briefs subagents, verifies and
   commits. A spawn costs a written brief, the subagent's boot (about 28,000
   tokens before it reads anything) and a harvest turn. The main agent makes a
   change itself only when ALL of these hold: at most 20 changed lines, in at
   most 2 files it has already read this session, no further exploration, and
   one command proves it. Anything larger is delegated. A main session running
   FABLE delegates even those: a cheaper tier can do them, and every fable turn
   costs more than the spawn.
2. INVESTIGATION ENDS IN A BRIEF OR IN A SMALL CHANGE. The moment you finish
   reading the code and know what the fix is, apply point 1's test. If it
   fails, you are at the delegation boundary: write the understanding down
   (file:line, root cause, the change to make, signatures, patterns to match,
   acceptance) and spawn. The diagnosis being yours does not make a large
   change yours.
3. EVERYTHING BEYOND POINT 1 IS DELEGATED, to as FEW subagents as the work
   allows, at the CHEAPEST tier that can do the piece. Size each assignment so
   the subagent typically finishes under about 400,000 tokens, and never split
   below what that needs: every extra spawn pays the boot cost again. Pick PER
   SUBAGENT by what that piece needs, never once for the whole stage, and
   default downward: HAIKU (`model: haiku` on loom-software-engineer) for
   mechanical edits such as a rename or a config value; codex gpt-6-luna for
   boilerplate, scaffolding, and simple unit tests; SONNET
   (loom-software-engineer) or codex gpt-5.6-terra for common implementation and
   integration tests — this is the default lane and most work belongs here; OPUS
   (loom-senior-software-engineer) for mainstream architecture and algorithm
   implementation; FABLE only for visual/UI design, a bug that survived a
   delegated fix attempt, or extremely challenging algorithmic design. Codex
   tiers (effort xhigh, via loom-codex-forwarder) exist only on stages listing
   codex in implementers AND when the codex CLI + plugin are installed;
   otherwise that work goes to sonnet (loom warns at startup when a stage lists
   codex it cannot use). Verification NEVER delegates - the orchestrator
   verifies and commits. Spawn BY AGENT TYPE.
4. ESCALATE ON EVIDENCE, NOT ON HUNCH. Start at the cheapest plausible tier. A
   fix that failed ONCE against clear acceptance criteria moves up exactly one
   tier — sonnet to opus, opus to fable — with the failed attempt and its
   evidence in the new brief; never rerun the same tier on the same bug. "This
   feels subtle" does not justify escalation. When a cheap subagent's output is
   wrong, first ask whether the brief was detailed enough — a vague brief is an
   orchestrator failure, not evidence the tier was too small.
5. DEBUGGING OR REPEATED FAILURE → spawn a `loom-advisor` (fable) subagent:
   narrow scope, full detail supplied by the orchestrator, advice returned, no
   writes. Its diagnosis then feeds a sonnet or opus implementer per point 2.
   Do not let an implementer thrash on the same failure twice.

Keep each assignment within point 3's size; decompose via Rule 6 rather than escalating for
context headroom. The cheaper the tier, the fuller the brief: paths, `file:line` ranges,
signatures, patterns, steps, per-step acceptance, decisions settled, traps named. Never paste code
the worker can open; it reads the ranges at its own rate. Plan authors: rubric in
`/loom-plan-writer`.

## Plan version 2 stages

A stage of a version 2 plan must satisfy more than its acceptance criteria. Its signal names the
stage's frozen contracts, the review gate and any findings carried in from an earlier stage; this
section is the flow around them.

**Contracts** (standard stages with `contracts`). Loom ran a contract session before yours: it
wrote each contract test and the harness files, confirmed every contract test fails, and froze
them. Never edit a frozen file, and name the frozen files read-only in every brief.
`loom stage contracts show <stage-id>` prints the freeze record and the frozen files;
`loom stage contracts restore <stage-id> [--contract <id>]` copies the frozen content back into
the worktree. A contract that is itself wrong is disputed, never edited:
`loom stage dispute-contract <stage-id> --contract <id> --reason "..."`.

**The review loop** (standard and integration-verify stages):

1. Spawn a `loom-code-reviewer` BY AGENT TYPE for the stage diff. Only that type is recorded:
   when it stops, a hook records the `loom-review` block ending its final message as the next
   review round. A final message without a valid block records a malformed round, which counts
   for nothing.
2. Brief every re-review with the output of `loom stage review status <stage-id>`: the rounds,
   every open finding (own and carried) with its id, whether the latest round matches the
   worktree, and the files changed since that round. A re-review covers those files plus the open
   findings, and the reviewer lists each open id under `resolved` or `unresolved`.
3. Every finding blocks completion, whatever its severity; suggestions never do. Each finding is
   either fixed and re-reviewed, or disputed. It closes when a later round lists it under
   `resolved`, or when the judge rules it `dismiss` or `defer`; `uphold` leaves it open.
4. Dispute a round's findings in one command:
   `loom stage dispute-findings <stage-id> --finding <id> ... --reason "..."`. Every dispute
   sends the stage to adjudication and ends your session. A stage may file 3 disputes of each
   kind (findings, contract, integrity); one more escalates it to `NeedsHumanReview`.

**Review order (plan v2):** fix every finding, run the full gate, then run the final review round, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.

**Test integrity** (standard and integration-verify stages). Completion compares the stage's test
files with the base. Fewer test declarations or assertions in a language, assertion lines removed
or changed in a test file that existed at base (a moved line does not count), or a changed
`ratchet_files` entry each raise an event; `loom stage review integrity <stage-id>` lists them.
Revert the change behind each event, or dispute them together:
`loom stage dispute-integrity <stage-id> --event <id> ... --reason "..."`. An accepted event stays
accepted while it gets no worse.

**What `loom stage complete` checks, in order.** It stops at the first check that fails.

1. The acceptance criteria. A criterion whose test runner selected zero tests fails.
2. The goal-backward checks: `artifacts`, `wiring`, `wiring_tests`, the dead-code check and
   `reachable`. A stage whose only goal-backward check is `reachable` skips this step.
3. Standard stages with contracts: every frozen file matches its frozen hash, and every contract
   test passes.
4. Test integrity (standard and integration-verify).
5. Standard stages: the tests that reach the stage's changed code pass. A test loom cannot select
   or run only prints a note.
6. Integration-verify: every completed stage's `reachable` checks, re-run on the merged tree.
7. The review gate (standard and integration-verify): the latest well-formed round matches the
   current change fingerprint, and no finding, own or carried, is open.
8. The checks every stage gets: `after_stage` commands, unwired files, duplicate symbols and
   change impact. Integration-verify also re-runs every completed stage's `wiring` checks.

**Integration-verify.** Its signal lists every pending reviewer suggestion of the plan's stages,
with its id. Consider each one: implement it, or leave it pending for knowledge-distill to record.
Resolve each one you implement with
`loom memory resolve <id> --outcome implemented --reason "<what changed>"`. Integration-verify
never defers a finding: fix it or dispute it. A `defer` ruling on its dispute is turned into a
request for more evidence.

## Reference

**Stage lifecycle:** `WaitingForDeps → Queued → Executing → Completed → Verified` (also `Blocked`,
`NeedsHandoff`, `WaitingForInput`). Retry a failed merge: `loom stage merge <stage-id>`.

**CLI:** `loom run` | `loom status` | `loom stop` | `loom check <stage-id> [--suggest]` |
`loom knowledge sync` | `loom repair [--fix]`.

**Handoff** (`.loom/work/handoffs/YYYY-MM-DD-desc.md`, written by `loom handoff` per Rule 3):
`# Handoff: [Description]`, `**Stage**: [id]`, `## Completed` with file:line refs, `## Next Steps`
prioritized.

**References:** cite code as `src/auth.ts:45-120`, not "the auth file".
