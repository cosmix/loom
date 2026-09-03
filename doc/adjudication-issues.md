# Adjudication Autonomy Failure — Findings

**Date:** 2026-09-02
**Plan:** `doc/plans/DONE-PLAN-release-versioning-config-and-loom-dir.md`
**Stage:** `loom-dir-migration`
**Outcome:** a correct `accept` verdict was recorded and applied, and the run still
deadlocked. The stage has been sitting `Executing` with no agent since
`2026-09-01T23:58:10Z`, one dispute is unanswered and permanently invisible, and no
watchdog in the codebase will ever fire on this state.

This is an account of *why the autonomy failed*, not of the dispute itself. Five distinct
defects contributed; four of them are independent and each would have been sufficient on
its own.

---

## 1. What was supposed to happen

1. Stage agent hits a criterion it cannot satisfy, files a dispute, stage → `NeedsAdjudication`.
2. Daemon spawns an adjudication session in the main repo.
3. Adjudicator rules, records a verdict via `loom stage adjudicate`.
4. Daemon applies the verdict: amend the plan, re-queue the stage.
5. Daemon spawns a **fresh stage agent** against the amended criteria.
6. Repeat for any further disputes; the stage eventually completes without a human.

Steps 1–4 worked. Step 5 did not happen, and step 6 was made impossible at step 4.

---

## 2. Timeline (UTC), with sources

| Time | Event | Source |
| --- | --- | --- |
| 22:09:43.655 | Stage session `session-441e6f28-1788300583` created, PID 30829 | `.work/sessions/session-441e6f28-1788300583.md` |
| 23:55:22.197 | Dispute 1 filed (criterion 5), stage → `NeedsAdjudication` | `.work/orchestrator.log` `Filed a spooled dispute ... dispute_id=1` |
| 23:55:27.445 | Adjudication session `session-748416d6-1788306927` created, PID 56007 | `.work/sessions/session-748416d6-1788306927.md` |
| 23:55:34.000 | Last heartbeat written by the stage agent | `.work/heartbeat/loom-dir-migration.json` |
| 23:55:34.059 | **Dispute 2 filed** (criterion 4) | `.work/disputes/loom-dir-migration/2/request.md` |
| 23:55:39.332 | Stage agent's last activity; idle from here on | `session-441e6f28...md` `last_active` |
| 23:58:05.705 | Plan amendment v1 committed (acceptance[5] replaced) | `.work/plan_versions/audit.md` |
| 23:58:05.718 | `Forced stage status ... from=NeedsAdjudication to=Queued reason=adjudicator verdict applied` | `.work/orchestrator.log` |
| 23:58:10.984 | `WARN Adopting live session instead of spawning a duplicate agent stage_id=loom-dir-migration session_id=session-748416d6-1788306927` | `.work/orchestrator.log` |
| 23:58:10.996 | `ERROR Refusing to evict an already-tracked active session incumbent_session=session-441e6f28-1788300583 rejected_session=session-748416d6-1788306927` | `.work/orchestrator.log` |
| 23:58:10 → now | Nothing but `Failed to sync graph status for stage loom-dir-migration: Stage 'loom-dir-migration' is not ready (status: Executing)` every ~5s | `.work/orchestrator.log` |

Resulting on-disk state:

```text
.work/stages/<n>-loom-dir-migration.md
  status: executing
  session: session-748416d6-1788306927   <- the ADJUDICATION session, not a stage agent
  dispute_count: 2
  amendments_applied: 1
  review_reason: <dispute 2's text>

.work/disputes/loom-dir-migration/1/  request.md verdict.json verdict.md applied.marker
.work/disputes/loom-dir-migration/2/  request.md            <- no verdict, never scheduled
```

---

## 3. Defect A — session adoption does not discriminate by session type

`Orchestrator::adopt_live_session_if_present` (`loom/src/orchestrator/core/session_lifecycle.rs:32`)
asks the on-disk registry for live sessions belonging to a stage and takes the newest:

```rust
let live_sessions = crate::orchestrator::session_registry::live_sessions_for_stage(
    &self.config.work_dir, stage_id,
)?;
let Some(newest) = live_sessions.into_iter().max_by_key(|s| s.created_at) else {
    return Ok(false);
};
```

`live_sessions_for_stage` (`loom/src/orchestrator/session_registry.rs:101`, filtering at
`:131-139`) selects on exactly two things — `session.stage_id == stage_id` and
`status ∈ {Running, Spawning}`. It never looks at `session_type`.

An adjudication session is created by `Session::new_adjudication(&job.stage.id)` and is
written to `.work/sessions/` by `spawn_for` (`loom/src/orchestrator/adjudication/session.rs:116-137`),
carrying `stage_id: loom-dir-migration` and `session_type: adjudication`. It is therefore a
first-class match for the stage's own worker, and — created at 23:55:27 versus the stage
agent's 22:09:43 — it is always the *newest*, so `max_by_key(created_at)` picks it in
preference to the real agent.

**This directly contradicts the design that was written down.** `spawn_for`'s own docstring
(`session.rs:108-115`) says:

> Mirrors the merge-resolution spawn, except that the session is NOT registered in the
> orchestrator's `active_sessions`: the stage's own agent may still hold that slot, and the
> monitor's crash reporting is for stage execution, not for a judge whose exit is ordinary.

The intent — an adjudicator is not a stage worker and must not occupy the worker slot — was
honoured for the **in-memory** `active_sessions` map and ignored for the **on-disk** session
registry, which is what the spawn path actually consults. The invariant exists in a comment
in one module and is unenforceable from the other.

The user's framing is the right one: adoption itself must stay. It is the daemon's recovery
path for its own restart — `stage_executor.rs:196-206` exists precisely so a requeue after a
daemon crash does not put a second agent into a worktree that already has one. The fix is not
to remove adoption but to make it type-aware: only a `SessionType::Stage` session for that
stage may be adopted as the stage's worker.

**Fix:** filter by session type. Either give `live_sessions_for_stage` a type parameter, or add
a `live_stage_sessions_for_stage` used by the adoption path, leaving the unfiltered function
for callers that genuinely want every session (status rendering, attach). Adjudication,
knowledge and merge-resolution sessions must all be excluded from worker adoption.

---

## 4. Defect B — adoption mutates the stage, then abandons the spawn with no retry and no escalation

Having picked the wrong session, `adopt_live_session_if_present` writes it into the stage
before it discovers anything is wrong (`session_lifecycle.rs:46-53`):

```rust
if let Err(e) = self.update_stage(stage_id, |current| {
    current.assign_session(session_id.clone());
    if current.status != StageStatus::Executing {
        current.try_mark_executing()?;
        current.begin_attempt(Utc::now());
    }
    Ok(())
})
```

So `stage.session` is overwritten with the adjudication session id and the stage is walked
`Queued → Executing`. Only afterwards does `insert_active_session` (`:253`) refuse the
registration, because the original agent still holds the in-memory slot:

```rust
if let Some(existing) = self.active_sessions.get(stage_id) {
    tracing::error!(... "Refusing to evict an already-tracked active session");
```

`insert_active_session` returns nothing, and `adopt_live_session_if_present` returns `Ok(true)`
regardless — the caller at `stage_executor.rs:205` reads that as "handled" and returns without
spawning. The two guards are individually defensible and jointly produce a state neither
anticipated:

- the stage is `Executing`, so the scheduler will never consider it again;
- `stage.session` names a session that is not a stage worker and will exit on its own;
- no error is surfaced to the operator beyond one ERROR line in a log;
- no retry is scheduled and no `Blocked`/`NeedsHumanReview` escalation is made.

The refusal at `:253` is the one place that *knows* the adoption was wrong (it can see the
incumbent is a different, already-tracked session), and it cannot tell its caller. It logs at
ERROR level and returns `()`.

**Fix:** two parts.

1. `insert_active_session` should return a result, and `adopt_live_session_if_present` should
   roll back the stage mutation (restore prior status and session) and return `Ok(false)` or an
   error when the incumbent is a different session — an adoption that could not complete must
   not leave the stage claiming it did.
2. An adoption that fails on a stage that has just been re-queued by a verdict is an
   infrastructure fault, not a no-op: it should escalate the stage (`Blocked` with a reason, or
   `NeedsHumanReview`) so it appears in `loom status` rather than dying in the log.

---

## 5. Defect C — applying a verdict re-queues the stage while other disputes are still unanswered

This is the bug behind "the second dispute should've been raised as well".

`job_for_dispute` (`loom/src/orchestrator/adjudication/mod.rs:108-138`) gates every pending
dispute on the stage's current status:

```rust
if stage.status != StageStatus::NeedsAdjudication {
    return None;
}
```

`disputes_awaiting_session` (`:82-103`) additionally hands out at most one adjudicator per
stage per pass, and `claim_session_slot` (`:157-169`) refuses while a live adjudication session
exists. Those two are correct and worked: between 23:55:34 and 23:58:05, dispute 2 was
correctly deferred because dispute 1's adjudicator was live.

The break is on the apply side. `apply_accept`
(`loom/src/orchestrator/adjudication/apply.rs:152-176`) ends with:

```rust
let _ = feedback::clear_feedback(work_dir, &stage.id);
transition_to_queued(stage)
```

It moves the stage to `Queued` unconditionally, without asking whether any other dispute on
that stage still lacks a `verdict.md`. The moment that landed at 23:58:05, dispute 2 failed the
`status != NeedsAdjudication` gate and has failed it on every poll since. It has a `request.md`,
no verdict, and no path back — the only thing that could schedule it is the stage returning to
`NeedsAdjudication`, and nothing does that on its own.

`disputes_awaiting_session`'s own comment (`mod.rs:89-97`) shows the multi-dispute case was
understood — *"A stage can carry more than one unanswered dispute"* — but the design assumed the
stage stays in `NeedsAdjudication` across passes. The apply path is what violates that
assumption, and it does so silently.

Note the compounding effect: dispute 2's text was written into `stage.review_reason` by the
filing path (`loom/src/daemon/server/dispute.rs:200-206`), so the stage now carries a
human-readable reason for a dispute that the scheduler cannot see. That is the worst
combination — the information is present and the mechanism is not.

**Fix:** `apply_accept` (and `apply_reject`'s non-terminal paths, and
`apply_needs_more_evidence`) must count remaining unanswered disputes for the stage — a
`request.md` with no sibling `verdict.md`, which `scan_pending_requests` already computes — and
transition to `NeedsAdjudication` rather than `Queued` when any remain. Only the last verdict on
a stage re-queues it. A regression test should file two disputes, answer the first, and assert
the second still gets an adjudicator.

---

## 6. Defect D — the disputing stage agent is never retired, so a live session always suppresses the respawn

Filing a dispute changes stage state and nothing else. `handle_dispute` in
`loom/src/daemon/server/dispute.rs:199-206` increments `dispute_count`, zeroes
`evidence_rounds` and calls `try_request_adjudication`. The agent that filed it is left running.
What the agent is told (`loom/src/commands/stage/dispute_criteria.rs:140-146`) is:

> The stage is now in NeedsAdjudication. The adjudicator will issue a verdict; run `loom status`
> to monitor.

There is no instruction to stop, and no mechanism by which a verdict reaches a session that is
already sitting idle. `session-441e6f28` has been `status: running` with `last_active`
23:55:39 ever since.

This matters beyond untidiness, and it is worth stating plainly: **even with Defect A fixed,
this deadlock still happens.** With type-aware adoption, `live_sessions_for_stage` would have
returned the original *stage* session — still alive, still a valid adoption target — and
`adopt_live_session_if_present` would have adopted it and returned `Ok(true)`, again without
spawning. The stage would sit `Executing`, bound to an agent that is idle and has no idea the
criteria changed. The failure would look identical and be harder to spot, because the session
id would be plausible.

So the verdict-apply path needs an explicit answer to "what happens to the agent that filed the
dispute?". Two coherent designs:

- **Retire it.** The disputing agent's turn is over: write its handoff, kill the session, clear
  `stage.session`, then re-queue. The successor reads the amended criteria from a clean start.
  This matches how the ceiling backstop already handles a takedown
  (`event_handler/recover_hung.rs:22-24`: "write the outgoing agent's handoff, kill every agent
  the stage owns, re-queue only once they are confirmed gone") and is the smaller change.
- **Signal it.** Deliver the verdict to the live agent as a new signal it is expected to poll
  for. This preserves the agent's context — worth a great deal at 401,836 tokens — but requires
  a wake-up mechanism that does not exist today and an agent contract that mandates polling.

Retiring is the correct default. The context saved by the second option is not worth inventing
an agent-wakeup protocol for, and an agent that has been idle for an unbounded period is not a
reliable executor of a criterion set it never read.

**Fix:** on verdict apply, if `stage.session` names a live session, retire it (handoff, kill,
clear the pointer) before `transition_to_queued`. Re-queueing a stage whose `session` field is
still populated is the same trap already recorded in `doc/loom/knowledge/concerns.md`
§ "Recovery: `retry --force` races daemon orphan-recovery on existing worktree" — that entry
noted `retry --force` leaves `stage.session` set and recommended clearing it. The same root
cause has now produced a second, worse symptom; the recommendation should be generalised into
an invariant: **no path may set a stage to `Queued` while `stage.session` is populated.**

---

## 7. Defect E — no watchdog covers this state

The daemon has exactly one mechanism for noticing a stage that is `Executing` with nobody
working: hung-session detection (`orchestrator/monitor/detection.rs:300-360`) feeding
`on_session_hung` → `recover_stalled_stage` (`event_handler/recover_hung.rs:57-110`). It cannot
fire here, for both sessions, for two different reasons.

`.work/heartbeat/loom-dir-migration.json` is a single per-stage file, and it is owned by the
original agent:

```json
{
  "stage_id": "loom-dir-migration",
  "session_id": "session-441e6f28-1788300583",
  "timestamp": "2026-09-01T23:55:34.000Z",
  ...
}
```

- **For `session-441e6f28` (the real agent, genuinely silent since 23:55:34):** the heartbeat is
  its own and is stale, so detection *will* report it `Hung`, and past three response budgets
  (`is_stall_escalation`, `monitor/hung_latch.rs:39-42`; 3 × `subagent_timeout_secs` = 3 × 1800s
  = 90 minutes) it escalates to `recover_stalled_stage`. That function's first guard
  (`recover_hung.rs:88-92`) is:

  ```rust
  if stage.session.as_deref() != Some(session_id) || stage.status != StageStatus::Executing {
      return Ok(());
  }
  ```

  `stage.session` is `session-748416d6` — the adjudicator, written there by Defect B — so the
  ids do not match and recovery declines silently. The guard is right in general; it is
  defeated by a corrupted `stage.session`.

- **For `session-748416d6` (the adjudicator now masquerading as the stage's worker):**
  `check_session_hung` (`monitor/heartbeat.rs:220-243`) looks up the stage's heartbeat, sees
  `heartbeat.session_id != session_id`, and returns `NoHeartbeat` — "stale heartbeat from a
  previous session". `NoHeartbeat` produces no event at all
  (`detection.rs:356-359`). So this session can never be reported hung.

Both paths are individually reasonable and together leave the state uncovered. **This is not a
slow recovery; it is a permanent deadlock.** The run will sit here indefinitely, emitting one
`Failed to sync graph status` warning every five seconds.

**Fix:** add a coherence check to the daemon's poll — a stage that is `Executing` while
`stage.session` names a session that is absent, not `Running`, or not of type `Stage` is
incoherent by construction and should be escalated (or repaired) rather than ignored. That
single check would have caught this within one poll, independently of A–D. It also belongs in
`loom repair`.

---

## 8. Defect F — `loom status` reports the deadlock as healthy progress

```text
●  loom-dir-migration [opus]  1h51m · 🔄 · PID 56007
```

PID 56007 is the adjudication session. The elapsed time is the stage's, the PID is a judge's,
and the spinner implies work in progress. Nothing in the display distinguishes "an agent is
working" from "the stage points at a session that was never its worker". The operator-visible
signal for a nine-stage plan that has stopped dead is a green dot.

`loom status` should render the session type it is reporting, and should mark a stage whose
`Executing` state fails the coherence check in §7.

---

## 9. Summary of fixes, in priority order

| # | Defect | Fix | Where |
| --- | --- | --- | --- |
| 1 | C | Only the last unanswered dispute re-queues; otherwise stay in `NeedsAdjudication` | `orchestrator/adjudication/apply.rs:152-176` |
| 2 | D | Retire the disputing session (handoff, kill, clear pointer) before re-queueing; invariant: never `Queued` with `stage.session` set | `adjudication/apply.rs`, `commands/stage/skip_retry.rs` |
| 3 | A | Adoption filters to `SessionType::Stage` | `session_registry.rs:101-152`, `session_lifecycle.rs:32-71` |
| 4 | E | Poll-time coherence check: `Executing` implies a live stage-type session | `orchestrator/core/recovery.rs`, `commands/repair.rs` |
| 5 | B | Failed adoption rolls back its stage mutation and escalates | `session_lifecycle.rs:46-71`, `:253-262` |
| 6 | F | `loom status` shows session type and flags incoherent `Executing` | `commands/status`, viewer |

Fixes 1 and 2 are what restore autonomy for the case actually hit. Fix 3 is what the design
already claimed to do. Fix 4 is the backstop that would have made any of the others survivable.

---

## 10. Note on the disputes themselves

Both disputes are, on the evidence, well-founded — the autonomy failure is in the machinery, not
in the agent's judgement.

- **Dispute 1 (criterion 5, `./hooks/tests/run-all.sh`):** accepted. The file is mode `100644`
  in all 16 commits of its history; `./hooks/tests/run-all.sh` exits 126. `bash hooks/tests/run-all.sh`
  exits 0 with `Results: 51 passed, 0 failed`. Amended.
- **Dispute 2 (criterion 4, the workspace `cargo test`):** accepted. The 14 named `hooks_*` tests
  fail identically on pristine `main` (HEAD `2ddf76c4`, clean tree), so no implementation of this
  stage can pass the criterion. Their deny path needs a live-ancestor check that requires `ps`,
  which this sandbox denies (`(eval):1: operation not permitted: ps`). Criterion 4 now carries the
  14 as named skips; the patched criterion runs 3518 tests, 0 failures, filtering exactly 8 in
  `lib` and 14 in `integration`.

  The accept is a scope decision, not an endorsement. Where the process tree is unreadable the
  read, poll, spawn and subagent-verify guards silently stop enforcing, while the sibling gate in
  `loom_is_subagent` was deliberately fixed to fail closed via `kill -0` in this same branch
  (commit `ea96610d`). `loom_hook_deny_or_warn` (`hooks/_read_discipline.sh:252`) was left on the
  old truthy `is_ancestor` test. That inconsistency deserves its own fix and its own stage.

One further item, outside any amendment's one-stage scope: the identical
`./hooks/tests/run-all.sh` criterion appears again at
`doc/plans/DONE-PLAN-release-versioning-config-and-loom-dir.md:2332` for a later stage
and will fail there in exactly the same way.

---

## 11. Further defects surfaced while applying the fixes

Recorded from the repair session itself; each one made the recovery harder than it should have been.

### G. There is no path to record a verdict once Defect C has fired

`ensure_recordable` (`loom/src/commands/stage/adjudicate.rs:126-134`) refuses a verdict on a stage
that is not `NeedsAdjudication`:

```text
$ loom stage adjudicate --stage loom-dir-migration --dispute 2 --verdict-file .../verdict.json
Error: Stage 'loom-dir-migration' is Executing, not NeedsAdjudication, so no verdict can be
recorded against it.
```

The guard is correct in isolation — it is one of the four that stop a disputing agent
self-approving. But combined with Defect C it is a trap: the stage left `NeedsAdjudication` because
another dispute's verdict was applied, and there is now no command anywhere that puts it back.
Dispute 2 could not be answered through its own machinery at all.

The substance had to be applied through `loom stage amend` (operator repair), which routes through
the same audited amendment path — snapshot, audit row, plan file, stage file — but leaves
`.work/disputes/loom-dir-migration/2/` with a `request.md` and no `verdict.md`. The adjudication
session's ruling is recorded in `verdict.json`, which is the draft, not the record. So the dispute
is materially resolved and formally still open.

**Fix:** fixing Defect C removes the need. If a manual path is still wanted, an operator command to
return a stage to `NeedsAdjudication` for a named unanswered dispute would close the gap without
weakening the self-approval guard.

### H. `loom stage amend` prints a fatal error and exits 0

```text
$ loom stage amend loom-dir-migration --field acceptance --op replace --index 4 --value "..."
Error: Failed to amend stage 'loom-dir-migration'
Caused by: Amendment would produce an invalid plan: ... acceptance criterion command too long
          (1933 chars, max 1024)
exit: 0
```

Nothing was written and the exit status says success. Any script or agent that checks `$?` rather
than parsing stderr will proceed as though the amendment landed. This is the failure class
`CLAUDE.md` Rule 13 exists for, in loom's own CLI.

**Fix:** return a non-zero exit status on amendment failure.

### I. The 1024-character criterion cap forces substring skips

The schema caps an acceptance criterion at 1024 characters. Criterion 4 with the 14 additional
tests named in full came to 1933. The workable form uses `--skip` substrings, which are silently
broader than full paths — two of the first draft's patterns over-matched **passing** tests
(`bash_side_cat` also caught `bash_side_cat_and_read_tool_share_the_same_read_ledger`;
`gated_untyped` also caught `ungated_untyped_spawn_warns_instead_of_blocking`), and nothing in the
tooling would have reported it: the criterion still exits 0, just over fewer tests.

That is the sharp edge. A criterion that silently stops testing things still passes. Catching it
needed a deliberate comparison of `filtered out` counts against `cargo test -- --list`.

**Fix:** raise the cap, or let a criterion carry a list of skips as structured YAML rather than one
shell string. Separately, a criterion whose `filtered out` count changes between runs is worth a
warning — that is the signal that a skip pattern widened.

### J. An adjudication session cannot perform the recovery it diagnoses

`loom stage reset` refuses while any live agent exists for the stage, and its notion of "live agent"
is the same unfiltered set as Defect A's:

```text
$ loom stage reset loom-dir-migration
Error: Stage 'loom-dir-migration' still has a live agent running
(session 'session-748416d6-1788306927' (pid 56007, tmux backend),
 session 'session-441e6f28-1788300583' (pid 30829, tmux backend)); refusing to reset.
```

The first of those is the adjudication session itself. And in `reset`
(`loom/src/commands/stage/state.rs:232-268`), `kill_live_agents` runs **before**
`update_stage(apply_reset)` — so `loom stage reset <id> --kill-session` invoked from the
adjudicator kills the adjudicator first and the reset never executes. The recovery has to be run
from a shell that is not one of the stage's sessions.

**Fix:** the same session-type filter as Defect A. An adjudication session is not an agent working
the stage and should not block a reset or be killed by one.
