# Subagent Orchestration

> Delegation model, defect reports, codex/pressure gotchas

## "I Wrote My Own Because I Could Not Reach Yours" Is a Defect Report

When a worker says it reimplemented something rather than calling it, that is a
duplication defect to fix at integration — **never a resolved decision**. In one round it
produced a second copy of a security-critical untrusted-content fence renderer, which is
exactly the rule that must not drift between surfaces. See
`mistakes/visibility-and-reachability.md` for why the worker could not reach the original.

## Related

- `mistakes/subagent-liveness-and-watch.md` — detecting alive/done/dead; loom subagents watch traps.
- `mistakes/verification-harness.md` — when every check fails at once, suspect the harness.
- `patterns/subagent-hierarchy.md` — choosing flat fan-out, a coordinator hierarchy, or a team.

## Subagent File Overlap Causes Lost Work

**Mistake:** Multiple subagents writing the same file leads to lost work (last writer wins).
**Fix:** Every subagent MUST have exclusive write access to its files. Use file ownership tables. If overlap is unavoidable, use one subagent or handle sequentially.

## Interactive Claude cannot be captured or made to auto-exit

**What happened:** A first cut of the `loom pressure` fixes assumed `claude -p` (or capturing Claude's stdout to a log) was the way to make Claude run one slash command and exit so the driver could proceed. Both are wrong for a subscription user.
**Why:** (1) `claude --help` states Claude runs in non-interactive mode "via -p, or when stdout is not a TTY, e.g. piped or redirected output" — so redirecting or capturing Claude's stdout silently switches it to the `-p` path and the session stops being interactive. An earlier version of this entry said that path bills pay-per-token API credits instead of the claude.ai subscription (anthropics/claude-code#43333); per the owner (2026-09-15) that never happened, and `-p` usage is not charged separately. (2) Feeding EOF on stdin (`< /dev/null`) does NOT let the task finish then exit — the REPL quits _before_ the agentic work completes (data loss), and an empirically-tested run also hit a workspace-trust dialog that `--permission-mode auto` did not skip. (3) There is no `--max-turns`/exit-when-done flag for interactive mode.
**Prevention:** For anything that must run as an interactive, visible session, Claude's stdout MUST stay a real TTY (foreground, uncaptured). Do not reach for `-p` or output redirection to "automate" it. Only ONE process can own the foreground TTY, so anything running concurrently (e.g. Codex) must be backgrounded with captured output.
**Fix:** Mirror the loom daemon's own model — the daemon never relies on Claude self-exiting; it SIGTERMs the session (`event_handler.rs` → `NativeBackend::kill_session`) once the agent signals completion via `loom stage complete`. `loom pressure` does the analog: inject a "`touch <marker>` as your final action" instruction via `--append-system-prompt`, poll for the marker, then SIGTERM the idle foreground session (manual exit as fallback).

## `loom pressure` codex "never starts" — it was invisible, not broken (2026-07-02)

**What happened:** The backgrounded Codex half of `loom pressure` was reported as "never starts (or starts and fails)". Investigation of the leftover logs (`/tmp/loom-pressure-codex-<pid>.log`) proved Codex ran fine in the recent runs: it triggered the `$pressure` skill, spent 170k–260k tokens, wrote its review next to the plan, and `/address` folded it in. Nothing was broken.
**Misleading signals:** (1) The driver printed NOTHING when codex spawned; the only codex UI was the wait-spinner, shown only when codex outlived the foreground Claude session — codex finishing first left zero terminal trace. (2) The codex report is deleted as final cleanup after all rounds, so no artifact survives a full run. (3) Every log contains a scary `ERROR rmcp::transport::worker … AuthorizationRequired` line even on successful runs — it is codex-side and non-fatal.
**Prevention:** Before diagnosing a `loom pressure` codex failure, read `/tmp/loom-pressure-codex-*.log` (one per driver invocation, overwritten per round) and check for `Wrote the pressure review to …` near the tail. Also note codex shares the driver's foreground process group: a Ctrl+C aimed at Claude SIGINTs codex too (`turn interrupted` in the log).
**Fix:** The driver now prints status lines — `→ codex review started in background (log: …)` at spawn, and after exit either `✓ codex review written → <report>` or a warning when codex exited cleanly without writing the report.

## A Fable Session Implemented the Fix It Had Just Diagnosed (2026-08-11)

**What happened:** a fable main agent investigated a bug, understood it, and then wrote the fix
itself instead of delegating — mainstream Rust edits at the most expensive tier available.

**Why:** the delegation rule was framed as "ORCHESTRATION IS ALWAYS OPUS / the orchestrator does
NOT implement." A fable session does not read itself as "the opus orchestrator," so the sentence
that should have bound it appeared to describe someone else. The fable _implementer_ tier also
listed "major bugs", and an agent that has just diagnosed a bug will classify it as major — the
exception swallowed the rule at exactly the moment the rule mattered.

**Prevention:** watch for the transition from "I now understand the bug" to the first Edit call.
That boundary is the delegation point, not a continuation of the investigation. Understanding the
fix is what makes a cheap subagent viable, so the cheaper the fix could now be, the stronger the
pull to type it yourself. Scope guidance to the SESSION's model, never to a model name assumed
from the role.

**Fix:** `CLAUDE.md.template` hard stop 6 (DELEGATION) plus Engineering Discipline E (Cheapest
capable agent) and a rewritten Rule 7 Model allocation: the rule is now stated model-independently
("the main agent never implements — whatever model it runs"), investigation is defined as ending
in a brief, the fable exception is narrowed from "major bugs" to "a bug that survived a delegated
fix attempt", and escalation requires evidence (a failed attempt), not a hunch.

**Follow-on (same day):** that template-only edit left every OTHER copy of the doctrine stale, and
the copies are not equivalent in how loudly they complain. `tests_doctrine.rs` then pinned BLOCK-A and
BLOCK-B byte-for-byte across `CLAUDE.md.template` and `skills/loom-plan-writer/SKILL.md` (today:
`skills/loom-orchestration/SKILL.md` and the plan-writer skill), so those two failed the build
immediately. The copies that say the same thing in DIFFERENT words are pinned
by nothing: the runtime signal prose in `orchestrator/signals/cache.rs` and
`signals/format/sections.rs`, the `Implementer::Claude` doc comment, and the knowledge summaries in
`patterns.md`. Those still told every spawning orchestrator "fable (major bugs, …)" — the exact
exception the change existed to close, on the surface an agent actually reads at run time. Editing a
doctrine block means `rg` for a distinctive phrase of the OLD wording across `loom/src`, `skills/`,
`agents/`, `loom-hooks/` and `doc/loom/knowledge/` before committing; a green `tests_doctrine` proves the
two pinned surfaces agree, not that the doctrine is consistent.

**Superseded wording (2026-09-19, doctrine-surfaces stage):** the "never implements — whatever
model it runs" text above is retired (it is in `RETIRED_PHRASES`, `tests_doctrine_blocks.rs`). BLOCK-B
now lives in `skills/loom-orchestration/SKILL.md` Rule 7 and `skills/loom-plan-writer/SKILL.md`, no
longer in `CLAUDE.md.template`, and its point 1 reads "DELEGATION IS A COST DECISION: TOKENS TIMES
MODEL TIER": the main agent may make a change itself only when it is at most 20 lines in at most 2
files it has already read, needs no further exploration and is proved by one command; a fable main
session delegates even those. The lesson above stands (the diagnosis-to-first-Edit boundary is the
delegation point); the size test is what decides whether a small edit crosses it.

## An Audit Proposed an Orchestrator Context Budget and a Hard Delegation Gate; Both Were Wrong (2026-09-18)

**What happened:** a transcript audit (the 2026-09-18 improvement-findings report, kept outside git) read
"keep the context per task under 250k" as a budget for the stage's main session and proposed a
250k working budget that redirects the orchestrator, plus a hard deny on any main-agent source
edit. It also blamed `commit-filter.sh` for the 62% verbatim-retry rate on attribution blocks and
proposed weakening the hook. The operator rejected all three.

**Why:** the audit optimised the number it could measure (main-session peak context) instead of
asking what the target governs. The per-task target governs the size of the work handed to a
SUBAGENT. The retry rate is the agent ignoring Rule 9 in favour of a harness reminder; the hook is
the only reason attribution does not reach every commit.

**Prevention:**

- Task size is set by subagent granularity: a subagent should typically finish under about 400k
  tokens, and every spawn pays a boot cost (median first request about 28k tokens in September
  2026), so too-fine splitting is waste as well. Group small tasks.
- Delegation is a cost decision, tokens times model tier. The main agent makes a very small edit
  itself when a spawn would cost more; a simple task a cheaper tier can do is still delegated when
  the orchestrator runs an expensive model.
- When a guard's block is retried verbatim, fix the instruction the agent is following. Do not
  loosen a guard that is catching real violations.
- `CLAUDE.md.template` is also used outside loom plans. Any restructuring keeps all guidance
  reachable in interactive sessions.
- Opus stays the default main-session tier for standard stages; the operator overrides per stage.

**Fix:** report sections 4.2, 4.6 and 4.7 rewritten to these decisions before the plan was authored.
