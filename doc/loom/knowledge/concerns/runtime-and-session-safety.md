# Runtime And Session Safety

>

## `evaluate_new_session` Fails a Working Spawn on a Benign `~/.tmux.conf` Warning

The rule "any stderr with exit 0 is a failure" is a plan mandate, pinned by unit tests and carried by
an explicit stage decision — so it was **deliberately left as-is**. But tmux prints `~/.tmux.conf`
deprecation warnings to stderr _while creating the session fine_, so one benign warning now: fails the
spawn, kills a **working** server via the abort path, and returns `Err` — which blocks the stage
(`FailureType::InfrastructureError`) instead of retrying on another lane. There is no
`.loom/work/terminal-backend-fallback` marker, so the consequence is a single Blocked
stage rather than tmux being disabled repo-wide.

The `has-session` probe that immediately follows is the authoritative signal and would distinguish the
two cases. Gating the stderr rule on that probe is a design call for the plan owner, not an
integration-verify defect.

## `loom attach` Overview Panes Are Live, Writable Agent Terminals

The overview's panes are full interactive attach clients (no `-r`), so a stray keystroke goes into a
live autonomous agent's input, and `C-b x` kills the **stage's** pane — the inner servers have no
`remain-on-exit`, only the viewer window does.

Left as-is deliberately: the plan mandates the exact pane string. If the overview is meant to be a
_viewer_ rather than N live terminals, adding `-r` to `attach-session` on the **overview path only**
(never on `loom attach <stage-id>`, which is the intentionally interactive path) is a one-word change.

## Overview Viewer Retirement

The overview viewer has a repository-specific retirement path used by explicit clean and init. Generic socket enumeration still excludes viewer sockets to avoid treating other repositories’ viewers as session sockets.

## PreToolUse File Guards Cannot Eliminate Path-Swap Races

The canonical `worktree-file-guard.sh` now covers Read, Write, Edit, Glob, and Grep; canonicalizes
paths; compares path components; and rejects both leaf and parent symlinks. This closes the concrete
absolute-path, common-prefix, and final-symlink escapes found in the security review.

The guard is still a check before a separate built-in tool performs the real open. A hostile process
can replace a parent path after the hook returns, so the guard cannot bind its decision to the inode
the tool later uses. Treat it as defense in depth; the host OS sandbox remains the authoritative
boundary. A race-free design requires a dedicated file-operation broker that performs traversal and
the actual open relative to an already-open worktree directory using no-follow semantics. Do not
describe the current hook boundary as TOCTOU-free.

## `loom attach <stage-id>` Dies With Its Stage

**Direct attach dies with its stage by design.** `loom attach <stage-id>` `exec`s into the stage's
OWN server (`commands/attach/mod.rs`), whose lifetime is the stage's (default `exit-empty`;
`completion_handler.rs` `kill_session` is only a backstop). The client gets `[exited]` on
completion; no follow/loop exists. Candidate design if this matters: make direct attach a
`select-pane` + `resize-pane -Z` focus on the long-lived overview viewer, so the operator is attached
to the viewer's lifetime, not the stage's (note `split-window` unzooms, and the build should reuse a
healthy viewer instead of `kill-session`-ing it). Daemon log for any attach/reconcile question:
`.loom/work/orchestrator.log`, level fixed by `RUST_LOG` in the shell BEFORE `loom run`.

## Orphan Adoption Only Runs at Daemon Startup

`adopt_orphaned_agents` is called from `recover_orphaned_sessions`, which
`orchestrator.rs` invokes exactly ONCE, at daemon startup — not on the poll loop, despite the
"every tick" phrasing that appeared in the brief that requested it. An agent orphaned mid-run
therefore stays invisible until the next `loom run`. That is enough for the incident it was written
for (a killed daemon is restarted by definition), and the spawn-time guard in `start_stage` closes
the duplicate-spawn hole independently, so this is a narrower reach rather than a hole. Making it
per-tick is a one-line addition to the scheduler loop; the pass is already idempotent and pinned as
such by a test, so the only question is cost — it scans `.loom/work/pids/` per Executing stage.

## Memory commands use shared state discovery

Memory commands resolve state through `commands::common::work_dir_path` (`commands/memory/handlers/work_dir.rs:76-77`), which delegates to `WorkDir::new` (`commands/common/mod.rs:23-34`). `fs/work_dir.rs:130-174` searches within the nearest repository and temporary-directory boundary, with direct workspace and state-root handling. `find_worktree_root_from_cwd` retains substring-based detection for its other callers; it is not the memory read resolver. Review those other consumers separately rather than changing read-only memory recovery based on this helper.

## `commands::memory` Tests Mutate the Process-Global Working Directory

Those tests call `env::set_current_dir` and rely on `#[serial]`, which only serializes a test
against OTHER `#[serial]` tests — anything unmarked in the same binary runs concurrently with a
foreign cwd in effect. In each test the `TempDir` is also declared AFTER its `EnvGuard`, so it is
deleted FIRST and there is a window where the process cwd points at a removed directory.

Neither of these caused the 77-failure incident (an ambient impostor `.git` did, see above), which
is why they were not changed. They remain a live hazard: any future test in that binary that
resolves a relative path while a sibling holds a foreign cwd will fail in a way that looks
unrelated to its own subject. Fix shape: inject the working directory instead of mutating the
process's, as `orchestrator/merge_lifecycle/tests.rs::cleanup_defers_while_the_cwd_is_inside_the_worktree_it_would_remove` already does deliberately for this exact
reason.

## Guard Hooks: Four Design Questions Deliberately Left Open

The adversarial review of `read-guard.sh`/`poll-guard.sh`/`_read_discipline.sh` surfaced four
behaviours kept AS SPECIFIED rather than patched, because each needs a spec decision, not a
one-line fix:

1. **The deny threshold may not fit CLAUDE.md's own workflow.** Five identical `git status`
   invocations in one session trigger a deny, yet CLAUDE.md's own commit workflow runs `git
status` before staging, again after committing, and again before completion — a real session
   following the documented workflow can plausibly hit the threshold.
2. **Ledger TSV keys can interleave across processes.** `_loom_ledger_append` uses the whole
   normalised command line as the TSV key with no atomicity guarantee beyond `PIPE_BUF`; a key
   longer than `PIPE_BUF` can interleave between two in-process subagents that both sanitise to
   `agent_id=main`.
3. **`loom_deny_enabled` is a line-oriented `grep`-style check**, so a TOML multi-line string
   VALUE that happens to contain the literal lines `[hooks]` and `deny_enabled = true` would
   enable the switch even though no real config intended it.
4. **`poll-guard`'s rule-2 `cat` branch is unreachable for any pre-existing `.loom/work` file**,
   because rule 3 (repeat-read escalation) fires first for files the ledger already has an entry
   for — effectively dead code on the common path.

None of these are fixed; each is a live behaviour a future stage should either ratify explicitly
or change with an accompanying spec decision, not patch as an incidental side effect of unrelated
hook work.

## `is_ancestor("1")` Cannot Distinguish "Not an Ancestor" From "Walked Off the Top of a Container"

`loom-hooks/_common.sh`'s `is_ancestor()` exits its walk-up-the-process-tree loop as soon as the
current pid becomes `"1"` or `"0"`, WITHOUT checking whether that final value equals the target
pid — so `is_ancestor(target="1")` is a guaranteed, deterministic `false` regardless of the real
process tree, even inside a container where PID 1 genuinely is an ancestor of everything. This is
useful as a test fixture (a non-ancestor `LOOM_MAIN_AGENT_PID` of `"1"` can never flake true), but
it is also a real edge-case correctness gap for any deployment where the loom main agent's PID
could legitimately be 1. Not fixed — recorded because the test-fixture use depends on the same
behaviour that makes it a latent bug elsewhere.

## Dispute Bookkeeping Gaps Around Escalation

`--approve` refuses while a live worker or merge resolver may write to the stage (`commands/stage/human_review.rs::refuse_live_writer`). A filing runs under the stage lock (`stage_service/dispute_store.rs::file_under_stage_lock`) and checks admission, budget and the transition on the locked stage before it publishes the request; `loom stage reset` closes open disputes (`commands/stage/state/loop_recovery/mod.rs`).

- **A crash between a filing's two writes leaves a request the stage does not name.** `file_under_stage_lock` publishes `disputes/<stage>/<n>/request.md`, then updates the stage; a failed stage write withdraws the request (`stage_service/receipts.rs::withdraw`), but a crash between the two writes cannot. The receipt, when there is one, makes the retried filing reuse the request, and the adjudication scan settles it unjudged once a later filing has named another. `daemon/server/dispute.rs` and `dispute_kinds.rs` are transport adapters.
- **Unverified: a survivor may block the fresh spawn.** A surviving session with no PID identity can read as dead to an approval probe. If it stays in `active_sessions` and `stage.session`, `insert_active_session` (`orchestrator/core/session_lifecycle.rs`) may refuse the fresh session; verify this against the current retirement and adoption paths before changing behavior.

## `failure_info` Outlives the Attempt That Set It

A status writer retires the reasons of the status it leaves: `Stage::try_transition` and `Stage::force_status_with_reason` (`models/stage/methods.rs`) call `retire_issues_of` (`models/stage/issue_history.rs`), which archives `failure_info` and `review_reason` into `issue_history` on entering WaitingForDeps, Queued, Executing, Completed or Skipped, archives `failure_info` on entering MergeConflict, archives a merge-conflict entry (with `merge.conflict_since` as its `detected_at`) on leaving MergeConflict, and drops a merge block labelled by the transition. Writers that record a reason for the new status write it after their transition. A failure overwritten without a status change goes through `Stage::replace_failure`, which archives the old one unless type and evidence are unchanged, so per-tick re-records add nothing. Explicit clears (`stage_service::apply_block`, `human-review --reject`, `loom stage retry --force`, `loom stage reset`, `clear_merge_block`) archive before clearing, so a stage Blocked with no `failure_info` still means `loom stage block` or a rejected review, never a crash (`orchestrator/core/event_handler/blocked.rs` relies on it).

Open gaps: re-blocking a crash-blocked stage (a legal Blocked to Blocked) archives the crash evidence but still cancels a pending auto-retry; `issue_history` is unbounded and ships whole in every status frame.

## Concurrent Daemon-Absence Probes Read Each Other as a Running Daemon

`inspect_lock` (`loom/src/fs/daemon_state.rs::inspect_lock`) proves no daemon runs by taking `flock(LOCK_EX|LOCK_NB)` on `orchestrator.lock`. Two probers at once (threads, or two `loom hook review-harvest` processes with no daemon) can each see the other's probe as a held lock, and `fingerprint::compute` then fails with `DaemonUnreachable`. The window is microseconds, and inside a stage the daemon answers on the socket so the probe never runs. Candidate fix: probe with `LOCK_SH|LOCK_NB` so probers share while the daemon's `LOCK_EX` still excludes them; check every `inspect_lock` caller and a daemon starting mid-probe first.

## Terminal History and Wheel Review Suggestions

Reviewer findings of the fullscreen-sampling and gesture-wheel work nobody implemented ([Terminal History](../architecture/terminal-backends.md#terminal-history), [Viewer wheel sensitivity](../architecture/web-terminal.md#viewer-wheel-sensitivity)).

- `loom/src/orchestrator/terminal/tmux/fullscreen.rs`: the worst case per poll is `SAMPLE_BUDGET` (2 s) plus `TMUX_PROBE_TIMEOUT` (5 s), and a wedged tmux server is reached on every poll. Skip a session for a few polls after a probe timeout.
- `tmux/history.rs`: a narrow race between processes: a teardown that loaded `fullscreen_seen = false` can write `Full` scrollback after a sampler recorded the flag and wrote a frame, leaving scrollback labelled `last_screen`; re-read the flag just before the `Full` write. `save_last_screen` also rewrites the session file under the sessions-dir lock with an fsync on every poll even when `fullscreen_seen` is already set.
- `loom/src/status/data/collector.rs`: the doc comment on `build_stage_summary` wraps mid-sentence.
- `web/src/routes/stage.tsx` repeats the labels-or-"open terminal"/"Terminal" fallback that `stage-modal.tsx` has; one helper beside `historyLabels` would own it.
- `web/src/components/terminal/viewer-wheel.ts`: the pager never calls `reset()` on the normal buffer, so a terminal that flips from normal to alternate within 250 ms seeds the gesture with the leftover sub-line remainder (a fraction of a line).

## Run Lane Backlog (server)

Reviewer suggestions on `loom/src/plan_server/run/` that no stage implemented. Each is a cleanup unless marked as a defect.

- **Defect risk:** `abandon/sequence.rs` sets `LOOM_CLEAN_YES=1`, so `loom clean` deletes any branch it lists itself; a stage session still running after `loom stop` can commit between the second branch check and the deletion. Passing the confirmed list for `loom clean` to compare would close the window (section 11 accepts it as specified).
- **Defect risk:** the commit worker (`commit.rs`) is not registered with `ctx.shutdown()` as the start worker is, so a shutdown mid-commit lets git finish but the `done` frame may never be sent. `start.rs` registers a `ShutdownHook` per start and never removes it, and the abandon route runs on the connection thread with no drain hook.
- **Defect risk:** `dashboard.rs` holds the state mutex through up to 10 s of child startup, blocking `current_url` callers (the strip poll and every `GET /api/run/state`). The dashboard startup error is logged now; the request still waits.
- A wildcard bind gives the holder the `127.0.0.1` URL even from another machine; choose the bootstrap line by the requesting Host's interface (`dashboard_parse.rs`).
- `start.rs` reads the state view before `child::jobs(..).begin()`: admission and the job slot are not atomic against another job (a microsecond window).
- A workspace with stage or session files but no plan source is offered Start with `--clean`, the branch-deleting path `start.rs` refuses for `Stopped`; consider refusing when stage files exist (`state.rs`).
- `orphaned.rs::holds_markdown` lists the directory twice and skips a per-entry readdir error, yielding `false`, which offers a destructive `--clean`; `any_entry(dir, is_markdown)` would propagate the error. `state::orphaned` imports `start::relative_work_dir` while `start` imports `state::state_view`; move it into `orphaned.rs`.
- When tracked changes and the run-state entry both block, the 409 names only the run-state remedy.
- `commit_doc_changes` and `abandon/route.rs` send every line as `Stdout`: git's stderr (hook errors) loses its stream, and the abandon line step `loom abandon` differs from the `done` frame's step.
- Cleanups: one newtype for the `Weak<PlanContext>` handles beside `RunJobs` (`start.rs`, `commit.rs`; `commit::install` re-calls `install_jobs`); one doc-path predicate for `changes.rs` and `commit.rs`; `install_dashboard` runs on every request and its group kill duplicates `child.rs`; `validate_start` should use `ctx.runtime()`; reword the `Marker::Remove` doc (after a non-zero `loom clean`, worktrees may already be gone); `confirm_branch_deletion` is 56 lines (baseline row in `maintainability-baseline.txt`) and ignores `listing_error` on the CLI path.
- `PlanMessage::RunState` and `RunJob::Dashboard` are never produced; the web `run_state` subscription is dead wiring. Broadcast them or drop them.
- Tests: no route test asserts exactly one `done` frame for abandon/finish-abandon; the operator test spawns duplicate the `LOOM_*` scrub of `common.rs` (share one helper that also re-sets `LOOM_HOME`) and leak the `loom plan` child on a failed assertion (`KillOnDrop` as in `duties.rs`); only the abandon path of Codex Stop is tested, and the stubborn Codex is killed after the assertions.
- Spec: `doc/product/feat-planner/section_12.md` "Accepted limits" says the server compares `.claude/` before and after a Codex step; `config_guard` leaves out a real `.claude/worktrees/` and fingerprints symlinks plus what in-repo link targets serve. Add one sentence. `doc/product/feat-planner/section_11.md` still cites the deleted the pressure models and fallback modules.

## Pressure Backlog

Open items on `loom/src/pressure/` and `loom/src/plan_server/pressure/`.

- **Defect risk:** `codex.rs` reads the Codex start time on the first poll, up to 500 ms after spawn; capture the `ProcessIdentity` at spawn in `CodexRun` so an attempt reaped before the poll cannot be recorded against a reused pid.
- **Defect risk:** Codex runs in loom's process group; commands it starts in the background survive its exit and SIGTERM and can write configuration after the after-step snapshot. A process group per Codex run, signalled through the verified leader identity, or a re-compare at the next launch would close it. A failed `StartBlocks::block` write (ENOSPC) is only logged while `current()` still returns `Ok(None)`, so launches stay allowed; keep an in-memory block on `PlanContext` as a fail-closed fallback.
- **Defect risk:** `record.rs` `replace_plan` re-resolves the parent path components at rename time: a `doc/plans` symlink planted during the run redirects the rename (only onto an existing file of the same name). A dirfd-anchored `renameat` through `safe_fs` closes it. The inode re-check misses same-inode in-place edits during the write window; also compare length and mtime from the read-time metadata.
- The dialog's default rounds is hardcoded to 2 and ignores `[pressure] rounds`, which the terminal resolves through `resolve_rounds` (default 1).
- `commands/pressure/mod.rs` dry run lists the record step after the final report deletion; `finish_pipeline` writes the record first (section 11 says so). The frozen contract `pressure_pipeline_contracts.rs` pins the dry-run order, so contract and spec disagree.
- `record_offset` uses the last END marker line while the plan parser (`plan/parser/extraction.rs`) uses the first one after START; section-ending headings must start at column 0 with a space while the reader's `atx_heading` accepts up to three leading spaces and a bare `##`. Apply the same rules in both.
- A Stop pressed between the last `/address` marker and `finish_pipeline` is handled by `stopping()` checks in `driver/step.rs`; verify the window stays closed when changing that file.
- Duplicate code: Codex abort-reason strings, `TAIL_BYTES` and log-tail reading in `plan_server/pressure/codex.rs` and `commands/pressure/spawn.rs` (move into a shared `crate::pressure` helper); `commands/pressure/tests.rs` repeats tests of `crate::pressure` items that belong in `src/pressure/tests.rs`; `CodexJob` is `pub` with private fields and no constructor, and `spawn_codex_background` has no external caller; `starts_with_doc_plans` is `pub` with no caller outside `paths.rs`.
- `config_guard`: a link resolving to the repository root or an ancestor of `.git`/`.loom` walks the whole repository and raises false blocks; walk served directories only for links inside `.claude/`. The `walked.iter().any(starts_with)` probe is linear per schedule (O(n^2) for many links); a `BTreeSet` probed with each ancestor is O(depth log n). Add a test with `.claude` as a symlink to a directory holding `worktrees/`.
- `record/tests.rs`: the 0311 directory-permission test passes without the fix when run as root; skip it under `geteuid() == 0`. The temp+rename write drops uid/gid, xattrs and ACLs and breaks hard links; say so in the `record.rs` doc.

## Web Run And Pressure Backlog

Open items under `web/src/plan/run/` and `web/src/plan/chat/pressure/`.

- **Behaviour:** `dashboard-button.tsx` POSTs `/api/run/dashboard` from every browser on page load whenever `strip.plan` is set, starting `loom status --web` without user action, and a failing start blocks the server for up to 10 s on each strip change. The brief asked for it; `section_11.md` reads as starting on first use.
- **Behaviour:** Stop (`pressure-header.tsx`) and `useRunRequest.submit` have no in-flight guard, so a double click sends twice.
- **Behaviour:** `use-run.ts` `fetchRunState` has no stale-response guard (the post-action and output-frame refetches can apply out of order; a generation counter as in `use-pressure.ts` would close it); done frames' `exit_code` and `step` are dropped, so the dialog never says which step failed and a second job of the same kind appends to the previous lines; `runOutputAtom` keeps finished jobs' output, so a pending abandon shows lines from an earlier attempt.
- **Behaviour:** a failed open-time options refetch overwrites good options and unmounts the form (losing edits); `refreshPressureOptions` resolves even when a newer request made it discard its result, so `refreshed` can turn true while the atom still holds the pre-open options; a never-settling fetch leaves the dialog loading (add an `AbortSignal.timeout`). Between Codex finishing and `/address` starting, `run.step` stays `codex`, so the composer briefly says Codex is reviewing.
- Holder-only controls are hidden from non-holders, matching the spec and `w2-run-ui.md:52/90` but contradicting its lines 100-105 (show disabled with a reason); settle the brief conflict. "Take authoring to ..." names an action the page has no control for (`run-dialog.tsx`, `plan-run-actions.tsx`).
- Accessibility: `key={step}` remounts the abandon `DialogContent` and its close-autofocus handler focuses the trigger behind the modal before the trap pulls focus back; refocus the trigger only when confirmation is null. The run row lives in `header/activity-rows.tsx` (no run strip component exists); whether its updates reach a live region is unchecked, and nothing announces job completion. Cancel is natively disabled while an abandon is pending with no reason. The absolute close button can overlap a long `DialogTitle` at 390 px and `DialogFooter`'s `flex-col-reverse` puts confirm above Cancel while focus order is Cancel first (`components/ui/dialog.tsx`). `ConfigBlockNotice` mounts in both the composer and `plan-run-actions.tsx`, giving two landmarks and two confirm buttons on a written plan with the dock open.
- Wording and duplication: the "merge or push those branches first" notice also shows after cancelling a confirmation with no unmerged branches; the Finish-abandoning notice does not name `state.finish_abandon`'s plan; the `configChanged` paragraph repeats the reason `endLine` already shows; `pressureReason` duplicates `useStartReason`; `actions-pressure.ts` and `actions-run.ts` should type the store as `Store` from `@/state/store`; `socketHarness` is duplicated in `use-run.test.ts` and `use-run-subscription.test.ts`; `PlanSelect` reorders the list on every choice; `DashboardButton` is 51 lines (Rule 17); a compact header strip renders a full error panel for a dashboard failure; add a client-side `http://` check before `href`; `key={path}` in `config-block-notice.tsx` warns on a duplicate path; give `pressure-dialog.tsx` the `plan-run-actions.tsx` fallback so `{available:false, disabled_reason:null}` cannot leave Start enabled; with identical lines at the output cap `droppedCount` can come out lower than the real drop.

## Unverified Live Behaviour

The browser render of the pressure dialog, run dialog, abandon dialogs and run strip at 390 px and 1280 px in every theme, dark-theme contrast, Radix focus timing across the two abandon steps, a real `loom pressure` run with agent-team teammates (`teammateMode` unobserved), a real Codex Stop/grace/kill, and a commit through the dialog with the user's hooks and signing were not exercised: the review sandbox has no browser (no Unix sockets, no network). They need an operator pass after merge.

## Proposed Guards

- A brief-path check for codex units: `loom-hooks/codex-forward.sh` could reject a forward prompt whose `doc/`, `src/` or `tests/` paths do not resolve from the unit's working directory, or rewrite them absolute. Twelve notes in one plan hit the recurrence recorded in [codex-worker-briefing](../mistakes/codex-worker-briefing.md).
- A `loom plan verify` warning for a stage whose description asks for a "default" change, when the named `#[default]` already holds that value at the base.
