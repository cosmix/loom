# Adjudication Autonomy Deadlock

> An accepted verdict deadlocked the run: adoption by stage_id alone, requeue with unanswered disputes, a live disputing agent, and no watchdog.

## Adoption Matched Sessions by stage_id Alone

**What happened:** The adjudication session for stage `loom-dir-migration` carried the stage's own `stage_id` (`loom/src/models/session/methods.rs:59-65`), and the daemon's session-adoption path looked up live sessions for a stage by `stage_id` alone. The adjudicator, spawned after the real stage agent and therefore newer by `created_at`, was adopted as the stage's worker in place of the actual agent.

**Why:** `live_sessions_for_stage` filtered only on `stage_id` and a running/spawning status, never on `session_type`. Adjudication, knowledge, and merge-resolution sessions all carry the target stage's `stage_id` by design, so any of them is a first-class match for "the stage's worker" from that lookup's point of view.

**Prevention:** Wherever a stage's worker is looked up, filter by session kind. `stage_id` is not identity — a session belonging to a stage is not necessarily that stage's *worker*.

**Fix:** Added a typed lookup, `live_sessions_for_stage_of_type` (`loom/src/orchestrator/session_registry.rs`), driven by `worker_session_type` (`loom/src/orchestrator/coherence.rs`), which names the `SessionType` that counts as a stage's worker. `session_is_current_for_stage` (`loom/src/orchestrator/core/orphan_adoption.rs`) now excludes `Adjudication` explicitly.

## A Verdict Re-queued the Stage While Another Dispute Was Unanswered

**What happened:** Two disputes were filed against `loom-dir-migration` seconds apart. Answering the first dispute re-queued the stage unconditionally, which made the second dispute's `request.md` permanently unschedulable — nothing ever put the stage back into `NeedsAdjudication`.

**Why:** `job_for_dispute` only hands out an adjudicator while `stage.status == NeedsAdjudication`, and `apply_accept` transitioned the stage straight to `Queued` on every accepted verdict without checking whether a sibling dispute still lacked its own `verdict.md`.

**Prevention:** A stage can carry more than one unanswered dispute at a time. Any verdict-apply path must count the stage's remaining unanswered disputes before deciding where to transition, not assume its own verdict is the last one.

**Fix:** `requeue_or_hold_for_remaining_disputes` (`loom/src/orchestrator/adjudication/apply.rs`) counts `request.md` files with no sibling `verdict.md` and only re-queues once none remain; a Reject verdict's `NeedsHumanReview` state is never overwritten by a later sibling verdict.

## The Disputing Agent Was Never Retired

**What happened:** Filing a dispute changed stage state but left the disputing agent's session running and idle. When the verdict later re-queued the stage, adoption picked that same idle session back up as the worker — so even with stage_id-only adoption fixed, a stage would still be bound to an agent with no idea the criteria changed.

**Why:** `handle_dispute` incremented `dispute_count` and requested adjudication but never touched the filing agent's session. Nothing retired it, and nothing woke it with the amended criteria either.

**Prevention:** A verdict-apply path needs an explicit answer for "what happens to the agent that filed the dispute?" Leaving it alive and re-queueing the stage hands its successor a stale, indistinguishable-looking session instead of a fresh start.

**Fix:** `retire_disputing_agents` (`loom/src/orchestrator/core/event_handler/stage_takedown.rs`) runs before every verdict apply: writes a `HandoffOrigin::Retired` handoff, kills the session, confirms it is gone, and clears `stage.session`. Verdict application defers if any such agent survives the takedown.

## Failed Adoption Mutated the Stage and Returned "Handled"

**What happened:** Adoption wrote the wrong session into `stage.session` and walked the stage to `Executing` before checking whether the in-memory active-session slot could actually accept it. The registration then refused (an ERROR log line, nothing else), and the caller read "adoption attempted" as "handled" and skipped spawning a real agent, leaving the stage mutation in place.

**Why:** The mutation and the registration check were split across two steps with no rollback between them, and the registration's failure signal (a log line) had no path back to its caller's return value.

**Prevention:** An adoption that cannot complete must not leave the stage claiming it did. Check before mutating, or roll the mutation back on failure, and escalate rather than leaving a silent no-op that looks identical to success.

**Fix:** the incumbent check now happens before mutation and escalates to `Blocked` on failure (`loom/src/orchestrator/core/session_adoption.rs`).

## No Watchdog Covered "Executing With Nobody Working"

**What happened:** The stage sat `Executing`, pointed at a non-worker session, for over ninety minutes. Neither of the daemon's two watchdog paths could fire: hung-session detection for the real agent's session compared against `stage.session`, which now named the adjudicator, so the ids never matched; the adjudicator's own session had no heartbeat file of its own to go stale.

**Why:** Both watchdogs keyed off `stage.session` or a per-session heartbeat file, and both assumed that field always names a live, correctly-typed worker. A corrupted pointer defeated both simultaneously, and the daemon logged only a benign-looking `Failed to sync graph status` warning every five seconds.

**Prevention:** Every non-terminal stage state needs either a live owner or a poll-time coherence check independent of that owner's own bookkeeping — never rely solely on paths that assume the pointer they are keyed on is already correct.

**Fix:** `reconcile_executing_stages` (`loom/src/orchestrator/core/coherence.rs`) runs each daemon tick and is also exposed through `loom repair`, checking directly whether an `Executing` stage's `stage.session` names a live, correctly-typed worker.

## The Completion Bridge Parsed a Wrapper the Harness No Longer Sends

**What happened:** For a 17.1KB verify output, the harness's actual hook input carried the full path in a structured `tool_response.persistedOutputPath` field, with `stdout` truncated to a prefix and no "Full output saved to:" wrapper text anywhere. `hooks/loom-control-complete.sh`'s persisted-output recovery only looked for that wrapper text, found none, and reported the verification marker missing — an operator had to complete the stage manually with `--no-verify`.

**Why:** The hook's fixture-driven recovery logic was written against an assumed shape of the harness's tool-output payload rather than a shape captured from a real transcript, and the harness's actual behavior had already diverged from it.

**Prevention:** Hook fixtures must be recorded from real transcripts (`~/.claude/projects/<project>/<session>.jsonl`, the `toolUseResult` field), never hand-written against an assumed shape.

**Fix:** `hooks/loom-control-complete.sh` now derives `PERSISTED_PATH` first from `tool_response.persistedOutputPath` / `tool_result.persistedOutputPath` via `jq`, falling back to the wrapper-text `sed` extraction only when that structured field is empty. Every downstream validity check on the resolved path — absolute, under `$HOME/.claude/projects/`, a `/tool-results/` segment, no `..` segment, a regular file, not a symlink — is unchanged.

## Four Silent Guards Made a 90-Minute Deadlock Invisible

**What happened:** Each of the four defects above declined to act without saying so anywhere an operator would see it: adoption logged an ERROR and returned success, the re-queue silently orphaned a dispute, the retired-agent gap left a session merely idle, and the watchdog compared against a pointer it had no way to know was wrong. `loom status` rendered the stage as a green, spinning, in-progress row the entire time, showing the adjudicator's own PID as if it were the worker's.

**Why:** None of the decline-to-act branches had a required "say so" path, and the status renderer had no way to distinguish "a stage-type worker is active" from "the stage's session field happens to be populated."

**Prevention:** Every decline-to-act branch in daemon logic should print to the daemon console rather than fail silently or log only at a level nobody watches live.

**Fix:** `loom status` now renders session kind and prefixes an incoherent `Executing` state with `INCOHERENT:` (`loom/src/commands/status/render/graph.rs`).

## The Judge Was Never Terminated

**What happened:** An adjudication session recorded its verdict with `loom stage adjudicate`, its prompt told it to stop, and the Claude Code process then sat open at its prompt until an operator closed the window by hand. Stage sessions are killed by the daemon on completion (`loom/src/orchestrator/core/completion_handler.rs`); judges had no equivalent.

**Why:** The verdict record carried no session id, so the daemon could not tell which judge wrote which verdict, and nothing in the apply path addressed the judge at all. While an idle judge lingers, `claim_session_slot` (`loom/src/orchestrator/adjudication/mod.rs`) refuses to spawn a judge for any further dispute on that stage.

**Prevention:** Every session kind the daemon spawns needs a recorded completion signal and a teardown that acts on it. Recording the verdict is the judge's completion signal; the record must name the session that produced it.

**Fix:** `DisputeVerdictRecord.session_id` (`loom/src/models/dispute.rs`) is filled from `LOOM_SESSION_ID` by `loom stage adjudicate`; after applying a verdict, `retire_adjudicator` (`loom/src/orchestrator/core/verdict_apply.rs`) kills that session, declares its record `Completed`, and removes its signal. A record without an id closes idle judges only when the stage has no unanswered dispute left.
