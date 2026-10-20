# Subagent Orchestration

> Delegation, defect reports

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

## Stage State Read From a Transcript Was Stale

**What happened:** after mining stage transcripts, the main agent told the operator a stage was still waiting on them, because the stage's last transcript message asked the operator to rerun completion and `ListAgents` listed a session with that name as busy. The stage had been merged some time before, and the plan was already in integration-verify.

**Why:** a transcript records what a session said, not what happened afterwards. Operator actions, later sessions and merges do not appear in it.

**Prevention:** before telling the operator anything about a stage's current state, read it from `loom status` or the stage file. Treat transcripts as evidence of past behavior only.

**Fix:** the operator corrected it.

## A Commit Piped Through tail Hid a Pre-Commit Failure (2026-10-02)

**What happened**: the orchestrator committed three groups with `git commit ... | tail -1`. The pre-commit rustdoc check rejected the first commit (a public doc linked a private fn), `tail` showed only a trailing line, and the next two `git add` calls stacked every group into one staged set. The failure surfaced only when `git log` showed no new commit.

**Why**: a pipeline's exit status is the last command's, and `tail -1` keeps whatever line happens to be last, so neither the status nor the output said the commit failed.

**Prevention**: check `git commit`'s own exit status (redirect its output to a file and test `$?`, or grep the output for `error`), and confirm with `git log --oneline -1` before staging the next group.

**Fix**: unstaged, fixed the doc link, committed each group with its exit status checked.

## An Integration-Verify Stage Spent 1.5M Subagent Tokens Re-Proving What the Sandbox Cannot Pass (2026-10-15)

**What happened:** integration-verify of the live-token-insights plan ran 3 full gate rounds and 5 review rounds and spent about 1.5M subagent tokens.

**Why:** (1) the full `cargo test` criterion cannot pass in the stage sandbox (about 60 environment failures: `ps`, the sysctl boot id, PTY, `/dev/fd`), and every opus verifier round re-proved that; (2) gate rounds ran in parallel with review rounds, which then produced more edits; (3) optional suggestions were implemented, which widened the review surface; (4) a guessed 1 s to 3 s deadline fix and a `bucket_secs` clamp that contradicted a base assertion (`sanitize_token_flow_tests.rs:76`, which test integrity forbids editing) each cost a round.

**Prevention:** settle first whether each criterion is satisfiable in the sandbox, and block or dispute early when not. Batch every fix, then run one gate and one final review. Implement only findings and the suggestions the signal lists. Read the existing assertions before briefing a behaviour change. Environment failures: [Seatbelt Denials Read as Build Warnings and Test Failures](sandbox-tooling-and-network.md#seatbelt-denials-read-as-build-warnings-and-test-failures-that-are-environment-not-code-2026-10-15).

## Repair contracts across shared subsystems

**What happened:** A repository-wide audit identified inconsistent lifecycle decisions, verification evidence, process retirement, source reading, provider accounting, and persistence transactions across consumers.

**Why:** Independently maintained representations allowed one consumer to enforce a condition while another silently omitted it.

**Prevention:** Assign disjoint implementation ownership and require the orchestrator to verify the complete tree before grouped Conventional Commits. Architectural repairs must route existing consumers through the shared implementation and retire the duplicate policy.

**Fix:** The repair contracts require complete authoritative stage snapshots; cause-aware block recovery; ordered prepared/applied amendment recovery; immutable checkout and complete versioned verification policy binding; one fail-closed assertion evaluator; bounded source hashing preserving exact blob bytes; shared owned-process retirement with real host containment; bounded JSONL spools with stable delivery identity; real exec thread/terminal evidence without invented turn IDs; bounded hook stdin; atomic signature-verified installation; transport-aware UI state; and shared provider normalization/inventory. These are implementation acceptance contracts, not a claim that verification has passed.

## Concrete boundary choices

**What happened:** Repairs require shared contracts spanning asynchronous and host-side consumers.

**Why:** Partial fixes can preserve the original race or trust gap in a second entry point.

**Prevention:** Stable spool identities need a receipt persisted atomically with a non-idempotent effect; a receipt written only after the effect leaves a replay window. Amendment recovery must reconcile audited snapshots in order. Verification pins a clean checkout before and after execution and binds the same revision at landing. Host confinement requires real Seatbelt or namespace containment and fails closed when unavailable.

**Fix:** Keep nullable provider turn identity and use genuine exec thread/start/terminal evidence. Require a trusted external signature verifier before executing a downloaded bootstrap candidate. Normalize provider message revisions as timestamp/ordinal-selected whole observations and retract prior live attribution. Let HTTP snapshots populate data without asserting WebSocket health, and refetch settings after concurrent mutations settle.

## Stop boundary and final scoped repairs

**What happened:** The operator stopped the broad bug repair run and requested a cold-start handoff and grouped commits, then authorized only B09 and A09 to finish in two high-effort workers.

**Why:** Many repairs existed without a passing integrated Rust gate; implementation and validated closure needed separate counts.

**Prevention:** Preserve unfinished changes and report explicit validation evidence. A scoped continuation does not resume the other findings. Serialize shared report edits and retain normal commit hooks.

**Fix:** Record 4 validated findings, 29 implemented but unverified, and 2 unfinished at the stop boundary in the original report. Two workers own only B09 and A09. Generated completion-output exceptions must yield to explicit stage input declarations; shell commands can still consume undeclared cache inputs. Commit independently verified dashboard and installer changes in a clean checkout, preserving the broader working tree.

## Verification ownership after a scoped stop

**What happened:** After stopping broad implementation and authorizing only B09/A09 completion, the operator explicitly requested three high-effort workers to verify finished uncommitted work.

**Why:** Persisted implementation needs independent evidence before any further closure or commits.

**Prevention:** Keep verification read-only, separate failures from fixes, and serialize heavyweight Rust jobs. A verification request does not authorize resuming all repairs.

**Fix:** Assign separate build/static, Rust behavior/security, and hooks/documentation verification workers; wait for the two active implementations before dependent checks. The orchestrator retains commit ownership and records outcomes in the original report.

## Verification expanded into owned repair work

**What happened:** The operator expanded the three verification assignments to include fixing discovered issues and returning verified work for orchestrator commits, and required the main agent to keep the original bugs report current.

**Why:** Read-only findings would leave completed-looking repairs unready to commit.

**Prevention:** Explicitly update file ownership when verification becomes implementation, serialize Cargo jobs, and keep one writer for the authoritative report.

**Fix:** The three workers now own disjoint Rust infrastructure, Rust verification/security, and hooks/maintainability repairs. The main agent alone updates doc/astra-bug-sweep-2026-10-15.md, records results as they arrive, and commits verified groups without attribution.

## End after verified commits and the authoritative report

**What happened:** The operator clarified that addressed, verified findings should be committed and the session should end once the original report is current.

**Why:** Continuing optional investigation after the agreed repair and verification assignments would exceed the requested stopping point.

**Prevention:** Finish the active assigned checks and necessary fixes, commit only supported results in logical groups, and record unresolved limitations without opening new work.

**Fix:** The orchestrator will commit the active workers’ verified changes, update the original report with exact results and remaining work, and end the session.

## Include knowledge changes in the final commit scope

**What happened:** The operator explicitly required the Loom knowledge updates to be committed along with the repair work and original report.

**Why:** Leaving recorded corrections only in the working tree would make the handoff incomplete.

**Prevention:** Include knowledge CLI edits and its regenerated index in the final documentation group, validate the corpus, and distinguish unrelated files.

**Fix:** The final commit scope includes doc/loom/knowledge and doc/astra-bug-sweep-2026-10-15.md. The unrelated pre-existing doc/RETRIEVAL-AB-METHOD.md remains untouched.

## Verification fixes are limited to finished repair work

**What happened:** The operator clarified that regression verification and fixes must concern the finished work awaiting commit, not resume unfinished findings.

**Why:** A broad runtime run exposed both regressions from completed repair changes and host execution failures; pursuing every failure would expand the stopping scope.

**Prevention:** Tie each corrective change to an already-finished repair under review. Document unrelated failures, environmental gaps and unfinished findings without implementing them.

**Fix:** Limit remaining checks to completed completion-proof, process and confinement repairs. The stronger B09 resolver-content proof remains deferred; finish the active bounded checks and required commit gates, commit supported work plus all knowledge/report updates, and end.

## Final sweep verification follows finished implementation

**What happened:** The operator requires the full test suite after all implementation is finished, a dedicated final verifier responsible for failures, and logically grouped Conventional Commits without generated attribution.

**Why:** Focused passes and broad runs made before later edits do not certify the final tree.

**Prevention:** Settle implementer edits first, give one verifier ownership of the full gate and corrective work, and commit only after absorbing its results.

**Fix:** Exhaustive gates use `cargo test --all-targets --no-fail-fast` under wall-clock and 2 GiB headroom watchdogs. Four test threads bound observed process/fork pressure; a serial exhaustive run is also authorized if diagnostics justify reducing host executable-assessment concurrency. Neither skips tests nor changes their own deadlines or internal concurrency. Reap superseded runs first. The coordinator preserves unrelated work, updates the original report and creates grouped commits.

## Delegate external reference research when requested

**What happened:** the primary agent began exploring Ponytail directly; the user directed a Sol or Terra subagent to explore the repository and fetch its skills.

**Why:** reference collection does not require the primary agent to spend its context on raw upstream material.

**Prevention:** assign external exploration and skill retrieval to the requested model; keep local integration design and implementation with the primary agent.

**Fix:** delegate Ponytail research to Sol and use its bounded findings and fetched artifacts.

## Keep guidance integration with the primary editor

**What happened:** the user clarified that Ponytail research belongs to a Sol or Terra subagent, while the primary agent decides and edits Loom guidance.

**Why:** the desired result is tighter existing guidance across executors and planning, rather than another instruction layer.

**Prevention:** keep the primary agent responsible for coherent edits across CLAUDE.md.template, AGENTS.md.template, relevant skills, loom-plan-writer, signal prose and delivery surfaces. Delegate upstream collection only.

**Fix:** retain the upstream research assignment and integrate by rewriting existing guidance; measure the prompt-size change and preserve execution boundaries.

## Keep guidance work within its requested scope

**What happened:** The guidance branch pre-commit hook reformatted an unrelated plan brief, and an unrelated macOS confinement test failure during pre-push prompted investigation beyond the requested guidance work. The operator corrected both scope expansions.

**Why:** Repository-wide hooks were treated as a reason to inspect or repair unrelated files.

**Prevention:** Inspect hook side effects before reporting completion, restore incidental edits, and report unrelated gate failures without turning a guidance task into test repair. Plan briefs are outside this task.

**Fix:** The incidental brief formatting was restored and excluded from every commit. No confinement implementation or test was changed. The operator cancelled the push and requested rebasing onto local main and merging there while preserving unrelated working-tree changes.

## A Brief Sent a Subagent to Docker, Which Started Docker Desktop Without the Operator Asking

**What happened:** to verify Linux-only fixes from a macOS host, the main agent briefed a subagent
to run the tests in a privileged Docker container. Running `docker` started Docker Desktop on the
operator's machine. The operator had not asked for it and objected.

**Why:** the main agent treated "`docker` is on PATH" as permission to use it. On macOS the CLI
launches a whole desktop application and VM, a heavyweight side effect on the operator's host.

**Prevention:** never start Docker Desktop, a VM, or any other host application or daemon, and
never brief a subagent to, without the operator's explicit go-ahead in this session. When a fix
can only be verified on another platform, say so and ask: offer CI as the verifier, or a container
if the operator agrees.

**Fix:** the subagent was stopped, and verification goes back to the operator's choice.

## `git rm --cached` Staged a Deletion in a No-Staging Task (2026-10-20)

**What happened:** a subagent told not to stage anything deleted a file with `git rm --cached`, which stages the removal.

**Why:** the reflex to keep the index in sync with the working tree.

**Prevention:** in a task that forbids staging, delete files with plain `rm`; `git rm` in any form touches the index.

**Fix:** `git restore --staged <path>`.
