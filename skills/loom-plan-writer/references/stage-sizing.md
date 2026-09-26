# Stage and Subagent Sizing

Read when: sizing a stage or its worker assignments, setting `subagent_timeout_secs`, or writing a stage description an orchestrator must decompose.

## Subagent response budget (`subagent_timeout_secs`)

Optional, seconds, default **300**. It is how long a stage may go without a heartbeat before the
orchestrator flags it, and the same number is written into the stage's signal so the session knows
the idle budget it is being measured against. It is NOT the `--timeout` handed to `loom subagents
watch`: that stays long (3600) so one background watch covers the whole wait, while this budget is
only the idle threshold death is judged against.

Set it from how long the work legitimately goes quiet, not from how long you hope it takes. A wide
mechanical sweep, a large test run, or a FOREGROUND codex run is one long tool call that emits
nothing while it works — codex stages in particular should raise it, since a foreground run posts no
intermediate output at all. A stage of small edits should leave it alone.

**Batch independent work.** Independent reads, greps and commands go in ONE message; never issue a
lone Read when the next two files are already known.

Checking on subagents is the `loom-orchestration` skill, `## Rule 6 — Subagents` ("Checking on
subagents: use one owned `loom subagents` wait, never a hand-rolled poll loop"), and that block is
canonical: it carries every exit code of the wait and what to do on each. Read it there instead of
restating it in a stage description. What bears on the plan: **subagents are ONE-SHOT** — brief
completely, run ONE background `loom subagents watch --worker <kind>:<id> [--worker ...] --timeout
3600`, distinguish exit 0 success, 2 deadline (not death), 3 failure or cancellation, 4 existing
wait (no second monitor), 5 unknown identity or evidence (never success), and 6 a hung worker
(stop it, re-delegate the remainder), harvest each terminal report once, and let the subagents
end; no stage needs a poll loop.

**Never message a finished subagent.** Each such message re-writes its entire conversation at the
cache-write rate. A follow-up is a FRESH spawn whose brief quotes the previous report.

Elapsed time alone is still never evidence of death.

Consequences for how you write a plan:

- **A stage OMITS `model`/`reasoning_effort` in its YAML by default, so the stage type's configured default applies** (opus for `standard`/`knowledge`/`integration-verify`, sonnet for `knowledge-distill` — `SKILL.md` Section 4); knowledge-distill also runs single-agent with no subagents. There is no per-stage subagent-model choice separate from this — the orchestrator's own model is this default/override chain; subagent model choice happens at spawn time, below.
- **The fable/opus/sonnet-or-codex-terra/codex-luna/haiku decision MOVES DOWN to the subagent level**, made by the orchestrator AT SPAWN TIME — not by the plan author in YAML. The orchestrator picks per subagent assignment, cheapest tier first: haiku for mechanical edits such as a rename or a config value; codex gpt-6-luna for boilerplate, scaffolding, and simple unit tests; sonnet or codex gpt-5.6-terra for common implementation and integration tests, the tier most work belongs to (which lane: `codex-implementers.md`); opus for mainstream architecture and algorithm implementation; fable only for visual/UI design, a bug that survived a delegated fix attempt, or extremely challenging algorithmic design (BLOCK-B rule 3; fable mechanics follow the block).
- **"Keep sonnet stages small" becomes "keep each subagent's assignment small."** A stage can be as large as the work genuinely requires; what must stay small is each individual subagent's task — that is what earns it a cheap model and keeps it inside its own context budget.
- **ESCALATION RULE: two failures on the same task ⇒ spawn a `loom-advisor` (fable) subagent, NOT a blind retry.** This replaces any earlier guidance to retry a failing subagent with a bigger model — diagnose first (narrow scope, full detail, advice returned), then re-dispatch with whatever the advisor recommends.

**The plan author still writes to sonnet-level detail — it now feeds the orchestrator's decomposition, not a sonnet agent's own literal execution.** Subagents follow what THEY are told literally; they don't infer intent, resolve ambiguity, or discover integration points. A vague stage description makes the orchestrator guess at decomposition, pick the wrong pattern for a subagent, or hand a subagent an underspecified task that produces stubs. Every stage description MUST include enough detail for the orchestrator to turn it into precise subagent assignments:

1. Exact file paths to create/modify (not globs).
2. Function/struct signatures to implement (name, params, return).
3. Existing patterns to follow — specific `file:line` ranges to read and replicate. **"Mirror X exactly" caveat:** name the property the new code must NOT copy and why. Mirroring is wrong the moment the new thing differs from X in a property X's code depends on (an auth-scoped cache reset, a store/provider the assertion needs, an ARIA role) — a literal executor copies the mismatch.
4. Step-by-step subtasks as instructions, not goals ("add field X to struct at line Y").
5. Integration wiring — which `mod.rs`/registry/route/test to update.
6. Error-handling approach — follow the target project's established stack; name which typed error
   callers match, where application-boundary context is added, and what is logged. Do not introduce a
   second general-purpose error framework for local convenience.

**If you cannot write that level of detail, that is usually a planning gap — go back and ground the seam (`SKILL.md` Section 1), then write it.** The orchestrator's own judgment can absorb some ambiguity a directly-executing subagent could not, but an underspecified stage still costs more in orchestrator decomposition time and subagent rework than the planning effort saves.

```yaml
# GOOD stage description — everything a sonnet subagent needs, handed to it
# by the opus orchestrator; small enough to be ONE subagent's task
- id: add-retry-logic
  description: |
    Add retry logic to HttpClient in src/http/client.rs.
    1. Create src/http/retry.rs with a RetryPolicy struct (max_retries: u32 = 3,
       base_delay: Duration = 500ms, max_delay: Duration = 30s) and
       delay_for(attempt) using exponential backoff w/ jitter — follow
       src/backoff.rs:12-35.
    2. Add retry_policy field to HttpClient (client.rs:45); wrap send()
       (client.rs:78-95) in a retry loop catching 429 and 5xx.
    3. Wire `pub mod retry;` into src/http/mod.rs.
    4. Use thiserror for errors, matching src/http/error.rs.
    Spawn ONE loom-software-engineer (sonnet) subagent with this same detail;
    verify and commit.
```

**Size each subagent's assignment by cost — decompose, don't up-model for headroom.** A subagent typically finishes under about 400,000 tokens, and every spawn pays a boot cost of about 28,000 tokens before it reads anything, plus a brief and a harvest turn. Two levers, in order: (1) group small tasks into one assignment and never split below what 400,000 tokens needs — an assignment likely to pass that is two assignments; (2) past about six assignments, decompose with a subagent hierarchy (`SKILL.md` Section 5) so the orchestrator (and any coordinator subagent) stays a THIN COORDINATOR at every level — workers burn their own (discarded) context and return compact summaries. **Delegation is a cost decision, tokens times model tier (BLOCK-B point 1):** a stage's main agent makes a small change itself (at most 20 changed lines in at most 2 files it has already read, proven by one command) and delegates everything larger. A stage whose bulk of implementation lands in the main session pays the main session's tier for every token of it; write the stage so that work is assigned to subagents.

**Size every worker task to finish under about 400,000 tokens. A task that needs more is two tasks — or a coordinator with two workers. A task far smaller than that joins another worker's assignment.**

**Bookend defaults:** knowledge-bootstrap defaults to opus at medium effort, integration-verify to opus at xhigh effort — both omit `model`/`reasoning_effort` unless deliberately overridden (`SKILL.md` Section 4). knowledge-distill is the one exception whose default model is not opus: sonnet at high effort, single-agent with NO subagents.

## Estimating a stage's context

Estimate against what actually accumulates in the orchestrator's own context: the brief itself,
every file the stage must read, each subagent's RETURNED REPORT, and the verification output all
land there and stay. A stage that fans out to six subagents pays for six reports — decomposing work
into more subagents lowers what any ONE of them holds, and raises what the orchestrator itself
accumulates reading their results back. Reading is the lever the plan controls: a range the
orchestrator reads stays in its context for every remaining turn, while the same range read by a
worker from its brief is paid once, at the worker's rate, in a context discarded when it reports.

A handoff is expensive: it discards a session with full context already loaded and hands the
successor a document to rebuild from, one that can run to tens of kilobytes. Weigh that cost at the
moment you size the stage.
