# Merge And Recovery Edge Cases

> Merge/retry edge cases

## BranchMissing Phantom-Merge Risk in merge_handler.rs (2026-04-16)

`handle_merge_session_completed` at line 97-103 treats `MergeState::BranchMissing` as a successful merge by calling `finalize_merge_resolution` which unconditionally sets `merged=true`. This violates the project invariant that daemon-side paths must never write `merged=true` without git ancestry verification.

Scenario: merge session dies, `check_merge_state` returns Conflict/Unknown, branch was deleted without being merged (e.g., manual `git branch -D`), code assumes "branch missing = cleaned up after merge."

Pre-existing issue, not introduced by the merge conflict session lifecycle fix. The `ProgressiveMergeResult::is_success()` method also still classifies `NoBranch` as success, inconsistent with `progressive_complete.rs` treating it as `Blocked`.

## BaseConflict Carve-out is Heuristic (2026-04-27)

`attribute_main_repo_merge` carves out `loom/_base/*` merges with a heuristic on the current branch name and on `SessionType::BaseConflict` session metadata. If a base-merge ever runs from a non-`loom/_base/*` branch (manual flow, future refactor) and no `BaseConflict` session is alive, attribution would tie the active merge to the stage whose branch HEAD shows up in `MERGE_HEAD` — leading to a spurious revert.

**Hardening path:** Tag base merges explicitly via session metadata (e.g., a marker file or distinct `SessionType::BaseConflict` always present during the base-merge window) and key the carve-out off that signal alone, not the current branch name. Until then, the heuristic is documented here so future work knows where to look.

## Recovery: `retry --force` races daemon orphan-recovery on existing worktree (2026-05-13)

**Observed:** `loom stage retry --force --context "..."` correctly set `integration-verify` to `Queued`, but on the next daemon poll the orphan-recovery routine in `orchestrator/core/recovery.rs:638-705` saw the (now-stale) session_id, found commits-ahead-of-base on the worktree branch, and immediately re-routed the stage to `NeedsHandoff` (commits_ahead path at `recovery.rs:668`). To the user, the stage looked stuck — they typed `retry`, it was ready for a second, then back to a handoff state with no agent activity.

This is a logically defensible design (commits exist, don't burn tokens redoing them), but the user-visible interaction is confusing. The "fix" — using `retry --force` a _second_ time after acknowledging the handoff — is undocumented in the recovery flow.

**What's needed (pick one or both):**

- `retry --force` should clear `stage.session` before saving, so subsequent orphan recovery doesn't treat the prior session as live and doesn't rerun its decision tree.
- Orphan-recovery should respect a recently-saved "retry intent" marker (e.g., a timestamp on the stage indicating user-driven retry within the last poll interval) and skip its commits-ahead reroute for those.

**Where to look:**

- `commands/stage/skip_retry.rs` (the `retry` command sets Queued at line 122 but leaves `stage.session` populated)
- `orchestrator/core/recovery.rs:633-707` (the orphan-recovery decision tree that re-routes to `NeedsHandoff`)

## Status Dashboard: `started_at` not refreshed on retry, stage appears "stale/orphaned" (2026-05-13)

**Observed:** After a successful `loom stage retry --force` that spawned a fresh session, the status dashboard rendered `integration-verify` as `19h4m · 🔄 · orphaned (stale)` for the duration of the new attempt. The number came from the original (long-dead) `started_at`; the new session was actually `Up About a minute` in podman and actively making tool calls.

**What's needed:** `stage_executor`'s spawn path (or `retry`) should reset `stage.started_at` to `Utc::now()` when a new session is created. The dashboard's "stale" heuristic should key off the new attempt, not the cumulative duration.

**Where to look:**

- `commands/stage/skip_retry.rs::retry` (where retry mutates stage fields)
- `orchestrator/core/stage_executor.rs:291-293` (`begin_attempt(Utc::now())` is already called here — confirm it's the only writer of `started_at` and that it's reached on retry).
- The "stale" indicator emitter — likely in `commands/graph/indicators.rs` or a dashboard renderer.

## Completion Broker: Nonce Burn After Transition Can Return Err for a Landed Completion (2026-08-09)

`daemon/server/control_complete.rs::handle_complete_stage` consumes the replay nonce AFTER
`update_stage(...).try_complete(...)`. This ordering is deliberate: a daemon crash between the
transition and the burn is benign — a replay of the same nonce is rejected by
`validate_active_identity` because the stage is no longer `Executing`, so no completion can be
duplicated, and (unlike the old burn-first ordering) none can be lost to a pre-effect burn.

The residual edge introduced by the reorder: a genuine IO failure on the replay-marker directory
(disk full, permissions) occurring after the transition makes the handler return `Err` for a
completion that durably landed. Self-healing in practice — the stage file is `Completed` on disk
and the daemon reads state from disk — but the caller observes a false negative for a succeeded
operation. If this is ever observed in the wild, split the response so callers can distinguish
"completed, replay marker failed" from "not completed".

## Merge Path Follow-Ups After the Silent-Unmerged Fix

Found while fixing the silent `Completed + !merged` outcome (`mistakes/phantom-merges.md`, last entry). Each is a separate change and was left as is.

- `loom stage merge` requires the cwd to be inside `.worktrees/` (`commands/stage/merge/preflight.rs::resolve_worktree_paths`). A stage whose worktree is gone but whose branch is unmerged has no loom command that merges it, and the hints printed by the daemon and `loom status` do not say to cd first.
- `verify_merged_true_or_revert` (`orchestrator/core/recovery.rs`) treats a git error from `verify_merge_succeeded` as "not verified" (`unwrap_or(false)`) and reverts `merged` to false, so a transient git failure can flip a merged stage to unmerged.
- `try_auto_merge` is 228 lines against the 50-line function cap and is ledgered at that size.

## A Retry Reset a Completed Stage's Branch, and the Stage Still Read `merged: true` (2026-09-19)

The `doctrine-surfaces` stage (`PLAN-loom-efficiency-and-acceptance`) committed four commits, then hit a
sandbox-setup-failure retry (stale installed hooks). The retry recreated `loom/doctrine-surfaces` at main's HEAD, and
completion recorded `merged: true` with `completed_commit` equal to main's HEAD, so nothing merged and the daemon
reported success. `git/worktree/operations.rs::create_worktree` reuses a branch with commits ahead of its base, so the
reset happened on a path that guard does not cover (the retry recreation, or a check that treats "no diff against
main" as merged). The root cause was NOT identified in this plan. Until it is, a stage that read `merged: true` must be
checked with `git merge-base --is-ancestor <stage tip> <target>` before dependants trust it, and the stage tip is
recoverable from `git fsck --no-reflogs` (see [phantom-merges](../mistakes/phantom-merges.md)). A retry that starts from
a completed stage should refuse when `commits_ahead_of(branch, base) > 0` and route to NeedsHandoff instead.

## Merge Resolver Spawn Loop: Open Gaps

Gaps around the [Merge Resolver Spawn Loop](../patterns/merge-and-recovery.md#merge-resolver-spawn-loop) that are known and unfixed:

- **No ancestry check before missing-branch routing.** `missing_branch_blocks_spawn` (`orchestrator/core/merge_handler/resolver_spawn.rs`) sends a stage whose `loom/<id>` branch is gone to review, even when its work already landed in the target. The outcome is safe against a phantom merge, but noisy; the manual step is `loom stage human-review <id> --force-complete`.
- **Unreadable stages fail closed.** Strict stage loading and the locked lifecycle snapshot reject malformed records before graph construction or terminal-state decisions; completed graph nodes do not exempt unreadable persisted state.
- **Liveness is judged by raw PID.** `find_live_merge_session_for_stage` (`orchestrator/signals/merge.rs`) calls `is_process_alive` on the recorded PID, so a reused PID can hold a stage until that unrelated process exits.
- **The gate stops resolvers before the guarded route.** `gate_holds_merge_stage` kills a live resolver, then routes the stage. A stage that moved on in between (for example, merged by `--resolved`) still loses its resolver. The window is narrow, and the branch is control-path-gated in any case.
- **Merge signal writes are not atomic.** `write_signal_file` (`orchestrator/signals/helpers.rs`) uses `fs::write`. A torn signal with no session record routes the stage to review. Only the daemon writes merge signals while it runs.
- **Transient spawn failures repeat a warning every tick.** While a spawn keeps failing (tmux, lock timeout), the daemon logs a spawn-failure warning on every poll until the cause is fixed. This follows from the no-cap decision.
- **A failed session save on the CLI resolver path can admit a second resolver.** `commands/stage/merge_resolver.rs` runs with no daemon and tracks nothing, so when its `save_session` fails the signal has no record, and the next liveness check reads it as stale. The daemon's paths, the spawn loop and `try_auto_merge`, keep a resolver whose record failed to save in `active_sessions`.
- **A tracked resolver with no session record can stay tracked forever.** If it resolves the merge, the monitor never fires for it, so it stays in `active_sessions`: `all_stages_terminal` stays false and it occupies a parallel slot.
- **Native window evidence needs wmctrl or xdotool on Linux** (`orchestrator/terminal/native/window_ops.rs`). Without them, a launched native resolver with no PID file reads as gone.

## Attribution Code Has Nothing to Attribute

`orchestrator/merge_attribution.rs` (`attribute_main_repo_merge`, `reconcile_main_repo_active_merge`, called from `orchestrator/core/recovery.rs`) and `commands/stage/complete.rs` `route_complete_conflict` rules 3-8 assume loom's merge leaves a `MERGE_HEAD` in the main checkout. Loom creates none now (`git/merge/mod.rs` `merge_stage` computes the merge with `merge-tree`), so for loom's own merges there is nothing to attribute. An operator's `MERGE_HEAD` attributes as `GlobalUnattributed`, which mutates nothing. The code is a removal candidate; its tests and the router arms go with it.

## Editor Saves Between the Stash and the Pop

Accepted residual: the reapply stashes, fast-forwards, then pops. An editor that holds an affected file open can save stale content in that window and overwrite the popped result. The backup ref holds the pre-merge content.

## The Target Can Be Checked Out Between the Location Check and the Ref Update

`advance_target` checks where the target is checked out, then runs `update-ref`. An operator who checks the target out in that window leaves R's index behind HEAD. This cannot be detected without racing again.

## Autostash Backup Refs Accumulate

A successful reapply keeps its backup ref under `refs/loom/autostash/` as a safety copy and nothing prunes them. Delete them with `git update-ref -d` once the work is confirmed.

## `human-review --force-complete` Requires No Operator Proof, by Design

It passes `MergeGate::Bypass`. Its authority rests on the capsule: every session kind denies writes to `.loom/`, so a stage agent cannot write the stage file the command changes first (pinned by `orchestrator/terminal/native/tests_capsule_denies.rs`, `every_session_kind_denies_writes_to_the_state_directory`). `human-review` is not relayed to the daemon, and the command checks no operator credential.

## Stage-Id Auto-Detection Fails Closed When a Tag Shares the Branch Name

`commands/common/mod.rs` detects the stage from `git rev-parse --abbrev-ref HEAD`. When a tag has the same name as the branch, git prints the ambiguous form, the stage is not detected, and the command falls back to asking for the stage id.

## A Refused Fast-Forward Listing Splits a Path Containing a Newline

The paths git names in a refused fast-forward are parsed line by line, so a path with a newline inside is read as two paths.

## A Defect in a Loom Gate Parks the Stage Until an Operator Acts

When one of loom's own completion gates cannot evaluate (its run errors), nothing in loom recovers the stage. This holds in every project loom runs, loom-on-loom plans included.

- **The stage agent cannot fix the gate.** `loom stage complete` runs the installed `loom` binary, so the worktree's copy of `src/verify` has no effect; in a loom-on-loom plan that directory is usually outside the stage's `files` as well.
- **The stage agent cannot dispute the gate.** `dispute-criteria`, `dispute-findings`, `dispute-contract` and `dispute-integrity` cover plan-authored checks, review findings, frozen contracts and integrity events. Loom's own gates (impact-selected tests, reachable re-verification, the checks every stage gets) have no dispute, so the adjudicator never sees them.
- **A block is untyped and terminal.** `loom stage block <id> <reason>` takes free text and clears `failure_info` (`commands/status/render/attention_model.rs:265-266`). The daemon auto-retries only `SessionCrash` and `Timeout` (`orchestrator/retry.rs::should_auto_retry`). A gate defect and a missing credential look identical, and both wait for a person. A manual `loom stage retry` re-runs the same deterministic gate and blocks again.

Observed on guard-core of PLAN-target-ref-guard (2026-10-02): blocked twice on the impact gate's E2BIG spawn error. See [[mistakes/verification-v2-delivery]], "The Impact Gate Failed a Stage on Its Own Spawn Error".

## Gate Defect Recovery: Proposed Long-Term Fix (Unimplemented)

1. **Every completion gate reports pass, fail or could-not-evaluate.** Could-not-evaluate is never a stage failure. The gate degrades per its design (D14's note deferring to integration-verify is the model) or, where no degradation is safe, routes the stage to adjudication with the error as evidence.
2. **A loom-internal block carries a type the daemon acts on**: a `--kind loom-defect` on `loom stage block`, or a `dispute-gate` command, recording the installed loom version. The daemon requeues such a stage when the installed loom version changes, and the adjudicator gains a verdict that waives one gate for one stage on evidence. A loom upgrade then releases every stage the defect parked, in every project, with no operator retry.

Scope spans the daemon (retry policy, block record), the CLI (block and dispute surface), the adjudicator (a gate-waiver verdict kind) and status rendering, so it needs a plan.

## A Stale `review_reason` Renders as the Block Reason

`commands/status/render/attention.rs:95-96` prints `review_reason` as `Reason:` for every problem stage. A dispute sets it (`models/stage/methods.rs::try_request_adjudication`), and on guard-core neither the verdict nor the later `loom stage block` cleared it, so `loom status` showed the integrity dispute's text ("tightening only: execute shrank...") as the block reason. The real reason was only in the `Note:` line.

## `list_loom_branches` May Return Linked-Worktree Branches With a `+` Prefix

`list_loom_branches` (`loom/src/git/branch/operations.rs`) parses `git branch --list loom/*` and strips only the `*` current-branch marker. Git marks a branch checked out in another linked worktree with `+`, so a stage branch with a live worktree would come back as `+ loom/<id>`. Suspected from git's output format, not tested; the guard's stage-work listing avoids it with `for-each-ref --format=%(refname)` and strips `refs/heads/` itself. Fix direction: list with `for-each-ref` here too.
