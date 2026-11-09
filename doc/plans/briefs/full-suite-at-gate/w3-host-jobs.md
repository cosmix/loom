# W3 brief: parallel `loom check` runs in one session dedupe on the host

> **Status: applied in `d83c59b2` (2026-11-08). Historical record; do not execute.** The anchors and §4 sizes describe the tree before that commit; replaying §6 recreates `pending.rs` and `store/slots.rs` with an older design. The tree also has what this brief omits: reset captures both job ids (`loop_recovery/mod.rs::observed_host_jobs`, tests in `commands/stage/state_host_job_tests.rs`), and `cancel_if_id` cancels a waiting job named by its id. §5.10's fixture `["true","true"]` would fail (identical commands share a cache key); the tree uses distinct commands. Remaining work, including the test gaps and two small races found later, lives in `doc/plans/PLAN-full-suite-at-gate.md`, section "Residual work".

Plan: `doc/plans/PLAN-full-suite-at-gate.md`, section 3 and the acceptance bullets "Job tests" and "Subagent relay".

Goal: in an integration-verify stage the orchestrator and its reviewer subagents share one `LOOM_SESSION_ID` and run `loom check integration-verify [--criterion <n>]`, possibly at the same time. On one tree every request runs once, joins a job, waits behind one, or hits the pass cache. No request cancels another request of the same session on the same tree.

All paths are relative to the repository root. Line numbers are at the tree this brief was written against.

## 1. Findings

### 1.1 Item 4: a subagent's `loom check` relays today

The relay already works for subagents, so the plan's "Check first" item needs no code fix. It does need one regression test (section 5.9).

- **Hook registration.** Every session capsule registers `loom-relay.sh` for PostToolUse with matcher `Bash` (`loom/src/orchestrator/terminal/native/session_settings/contents.rs:22-23`). It is never registered globally (`loom-hooks/loom-relay.sh:13-14`).
- **The hook fires for subagent Bash calls.** This was observed directly. In the sidechain transcript of a worktree subagent (`"isSidechain":true`), a `hook_success` attachment named `PostToolUse:Bash` carries `LOOM relay: received memory 92d36162 (the daemon applies it within ~5s)`. The transcript is `~/.claude/projects/-home-dkaponis-src-loom--worktrees-home-and-shell/4e4bfb05-7fe8-46e6-b75f-7e3562f90dbc/subagents/agent-a7b195b99e4097381.jsonl`, dated 2026-11-04. The payload carries a non-empty `.agent_type` for a Task-spawned subagent (`doc/loom/knowledge/architecture/hook-system.md:111-112`).
- **The shell side drops only control kinds for a subagent.** `loom-hooks/loom-relay.sh:185-187` calls `drop_control_kinds`, and its list at `loom-hooks/loom-relay.sh:125` does not contain `check`. `loom-hooks/tests/loom-relay-kinds.sh:173-177` pins `expect check 'loom check s1' general-purpose`.
- **The Rust side does the same.** `loom/src/commands/hook/relay.rs:332-344` refuses only `kind.is_control()` from a subagent, and `RequestKind::Check` is not a control kind (`loom/src/relay/kind.rs:55-67`). Attribution comes from `agent_role` (`relay.rs:357-362`). The session proof `prove_session` (`relay.rs:226-250`) holds, because a Task subagent runs in the main agent's process tree, as the memory relay above shows.
- **The daemon admits it.** `loom/src/orchestrator/core/inbox_drain/apply.rs:53` refuses only control kinds from a subagent. `inbox_drain/check.rs:238-247` (`a_subagent_of_the_owning_session_is_queued_too`) queues a subagent's check.
- **The follow call is agent-agnostic.** `commands/verify_relay.rs:106-118` builds the `WaitSite` from `RelayContext` (`LOOM_SESSION_ID` and `LOOM_SCRATCH_DIR`), which a subagent's Bash inherits. `poll-guard.sh` does not count repeated `loom ...` calls (`loom-hooks/_progress-classification.sh:197-210`). `subagent-verify-guard.sh` has no matcher for `loom check`.
- **What is missing.** No Rust test relays a `check` ticket with `agent_type` set. Section 5.9 adds one.

Field verification, to run after the first integration-verify stage that uses this. It replaces nothing above; it confirms the acceptance bullet "Subagent relay".

```bash
# <proj> is the stage's ~/.claude/projects/<...worktrees-integration-verify> dir
rg -n -o 'LOOM relay: received check [0-9a-f]{8}' ~/.claude/projects/<proj>/*/subagents/agent-*.jsonl
rg -n -o 'Host job: (pending|passed|failed|stopped) \(check, [0-9a-f]{12}\)' ~/.claude/projects/<proj>/*/subagents/agent-*.jsonl
rg -n '"kind":"check".*"reason":"host job [0-9a-f]{32} (queued|joined|queued behind)' .loom/work/inbox/*/ledger.jsonl
```

If the first command finds a subagent's `Host job: requested` with no `received check` reply, the subagent's PostToolUse hook did not run. The fallback in that case is to add `RequestKind::Check` to `sweepable` in `loom/src/commands/hook/relay/sweep.rs`, so the main agent's next `loom check` call relays the leftover ticket. Do not build this fallback now.

### 1.2 Defect 1: one wait-state file per session, stage and kind (confirmed)

- `loom/src/host_verify/wait_state.rs:30-34`: `state_path` is `<scratch>/host-verify/<stage>-<kind>.json`.
- `wait.rs:166-170`: `drive` reads that one file, and `state_matches` (`wait.rs:189-194`) compares head, tree, `no_cache` and `criteria`. A mismatch relays a new request, and `write_state` (`wait.rs:276`) overwrites the file.
- Root cause: reviewers A on {1} and B on {2} share the session, so each call overwrites the other's state. Each follow call then relays a new ticket instead of following its own request.

### 1.3 Defect 2: an uncovered check cancels the running check (confirmed)

- `loom/src/host_verify/submit.rs:69-88`: when `covers` (`submit.rs:156-166`) is false, the request is a check and the current job is a check, the code falls to `end(&mut current, "superseded by …")`.
- This holds for a `Running` job and for a `Queued` one. A and B on one tree would cancel each other's job on every relay.

### 1.4 Defect 3: `--criterion` is not normalised (confirmed)

- `loom/src/cli/dispatch.rs:188` passes `args.criterion` (a `Vec<usize>` in argv order) to `commands/verify.rs:43` (`execute`). `execute` hands it unchanged to `session_relay::run` (`verify.rs:46`), which reaches `verify_relay.rs:115` (`WaitSite.criteria`).
- `state_matches` compares `Option<&[usize]>` by order (`wait.rs:193`), so `--criterion 2 --criterion 1` after `--criterion 1 --criterion 2` relays a new request.
- The local run builds a `BTreeSet` (`verify.rs:108-110`) and is unaffected.
- The concern bullet `doc/loom/knowledge/concerns/verification-v2-followups.md:153` cites `commands/attach/tests/wait.rs::classify_endpoint_failure_distinguishes_the_three_causes`. That is the wrong place: the comparison is `host_verify/wait.rs::state_matches`. The bullet is deleted in section 8.

### 1.5 Where the plan is wrong or incomplete

1. **"waits behind the running check".** A check that is `Queued` and not yet started is cancelled the same way, and two reviewers would thrash while both wait for a slot. The rule must cover any non-terminal current check.
2. **Same session only.** A judge's check or a merge resolver's check (`child_policy.rs:26-32`, `CachePolicy::Bypass`) must not be merged into a stage session's run, because the job's cache policy follows `record.session_id`. A check from another session on the same tree keeps today's supersede. Integration-verify reviewers share the orchestrator's session, so they are covered.
3. **The pending slot has more callers than `submit`.** The slot must be cleared by every cancel path, and it must be superseded along with the running job whenever that job is superseded. That includes a completion, freeze or verdict-completion request, which still supersedes a check.
4. **Fixture.** `host_verify/tests_complete_support.rs::Fixture` is the fixture for applying a completion: repository, plan, session, then `apply_finished`. It cannot drive queueing or the runner. Use the existing helpers instead:
   - `tests_submit.rs` (`state_root`, `request`, `subset`) for submission rules.
   - `tests_runner.rs` (`state_root`, `shell`, `PASS`, `tick_until`, `running`) for the runner.
   - `tests_child.rs::Fixture` for real criteria and the pass cache.
5. **"Check first: subagent relay".** This is already true (1.1). It needs a test, not a fix.

## 2. Design

### 2.1 Rules for a submitted request

`submit` evaluates these rows in order, under the stage lock. "Live" means not terminal.

| # | Stage state | Request | Result |
| --- | --- | --- | --- |
| 1 | no live current, no live waiting job | any | `Queued` (today) |
| 2 | no live current, live waiting job | any | promote the waiting job to current, then rows 3-6 against it |
| 3 | live current covers the request (`covers`, unchanged) | any | `Joined(current)` (today) |
| 4 | live current is not a check | uncovered check | `Refused(CHECK_BEHIND_RUN)` (today; a check never cancels a completion or contract run) |
| 5 | live current is a check with the same `head`, `tree_digest` and `session_id` | uncovered check | `Waiting`: the request widens the stage's waiting job, or creates it |
| 6 | anything else (new tree, other session, a completion, freeze or verdict completion over a check) | any | `Superseded`: cancel the waiting job, cancel current, queue the request (today, plus the waiting job) |

Union semantics in row 5:

- `criteria`: `None` when either side is `None`. Otherwise the sorted, deduplicated union.
- `no_cache`: the OR of both sides.
- `requests`: the request id is appended and capped at `MAX_REQUESTS`, as in a join.
- Only waiting selections are unioned. The running job's selection is never added. This is how "criterion 1 is not run twice" holds: A runs {1}, and the waiting job runs {2}. A waiting request for {1, 2} lists 1 again; the pass cache answers it when A passed it, unless the waiting job is `no_cache`.

Invariant: a waiting job exists only while it is `Queued` and the current job is a live check of the same stage, session, head and tree, or for at most one runner tick after that check ended.

### 2.2 On-disk layout

```text
<state root>/verification/<stage>/
  .lock              unchanged
  current.json       unchanged
  pending.json       NEW: the stage's waiting check, a JobRecord in state queued
  <job id>/record.json   the waiting job has its own directory and copy, like every job
```

- `pending.json` and `current.json` change only under `store::lock_stage`. There is one lock per stage and no nested lock.
- The job copy `<id>/record.json` is written before `pending.json` (as `save_current` already does), and it wins when the two differ. A crash between the two writes leaves the copy newer. A cancelled copy means the job was cancelled.
- The waiting job's directory has no `applied` marker, so `prune_jobs` keeps it (`store.rs:309-316`). `apply_retry::unapplied_of` skips it while it is non-terminal (`apply_retry.rs:56-60`). When it is cancelled, it is applied as a cancelled check, which changes no stage state.
- The waiting job gets its `submission_order` from `next_submission_order`, like every job.

### 2.3 Store API: new `loom/src/host_verify/store/slots.rs`

`store.rs` is 394 lines, so the code goes into a submodule, following the pattern of `store/order.rs` (`use super::*;`). `save_current` becomes a delegate, which makes `store.rs` shorter.

```rust
// store/slots.rs
const PENDING_FILE: &str = "pending.json";

/// Write `record` as `<job dir>/record.json`, then, when `slot` is set, as the
/// stage file `slot`; creates the job directory. Body moved from `save_current`.
pub(super) fn save_slot(work_dir: &Path, record: &JobRecord, slot: Option<&str>) -> Result<()>;

/// The stage's waiting check: `pending.json`'s record, replaced by its job
/// copy when the copy exists (the copy is written first and wins).
pub(crate) fn load_pending(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;
pub(crate) fn save_pending(work_dir: &Path, record: &JobRecord) -> Result<()>; // save_slot(.., Some(PENDING_FILE))
pub(crate) fn save_job_copy(work_dir: &Path, record: &JobRecord) -> Result<()>; // save_slot(.., None)
/// Remove `pending.json`; an absent file is fine. Call only under the stage lock.
pub(crate) fn clear_pending(work_dir: &Path, stage_id: &str) -> Result<()>;
```

- Edit `store.rs:94-107` to `pub fn save_current(..) -> Result<()> { slots::save_slot(work_dir, record, Some(CURRENT_FILE)) }`.
- Add `mod slots;` next to `mod groups; mod order;` (`store.rs:240-241`), and add `pub(crate) use slots::{clear_pending, load_pending, save_job_copy, save_pending};`.
- `load_pending` reads with `read_json(work_dir, &stage_rel(stage_id)?.join(PENDING_FILE), RECORD_MAX_BYTES)`, then calls `load_job_record(work_dir, stage_id, &pointer.id)?.unwrap_or(pointer)`.
- `clear_pending` calls `crate::fs::safe_fs::safe_remove_in_workdir(open_root(work_dir)?.as_raw_fd(), &rel)`, which treats ENOENT as success (`fs/safe_fs.rs:508-541`) and follows no symlink.
- `save_slot` keeps `refuse_unsafe` on every target, as `save_current` does now.

### 2.4 Waiting-job lifecycle: new `loom/src/host_verify/pending.rs`

Declare it in `host_verify/mod.rs` as `pub(crate) mod pending;`. Every function that changes state takes `&StageLock` as proof that the caller holds the lock.

```rust
//! The stage's waiting check (`verification/<stage>/pending.json`): a check of
//! the running check's session and tree that the running job does not cover.

/// The live waiting job; a terminal one (cancelled in its copy) is cleared.
pub(crate) fn live_locked(_lock: &StageLock, work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;

/// Make the live waiting job the current one when current is missing or
/// terminal. A waiting job that is already current (a crash after
/// `save_current`) only has its slot cleared. Returns the record that is
/// now current and live, if any.
pub(crate) fn promote_locked(lock: &StageLock, work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;

/// `promote_locked` for a caller without the lock; returns at once, taking
/// no lock, when `load_pending` finds nothing.
pub(crate) fn promote(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;

/// Cancel the live waiting job with `reason`: its copy turns cancelled first,
/// then `pending.json` goes. Returns the cancelled record.
pub(crate) fn cancel_locked(
    lock: &StageLock,
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Option<JobRecord>>;
```

`promote_locked` steps:

1. Call `live_locked`. If it returns `None`, return `None`.
2. Load the current record.
   - If current is live and is a different job, return `None`: the waiting job keeps waiting.
   - If current is this job, use it.
   - Otherwise, call `save_current(&waiting)` and use `waiting`.
3. Call `clear_pending`.
4. Return the record only if it is live.

`cancel_locked` writes the copy before it clears the slot. A crash between the two leaves `pending.json` pointing at a cancelled copy, and `live_locked` drops it. With the opposite order, a crash would leave a `Queued` copy that no slot names, and its waiter would wait forever.

`submit.rs::end` (`submit.rs:224-228`) becomes `pub(super)`, so that `cancel_locked` reuses it instead of copying it.

### 2.5 `loom/src/host_verify/submit.rs`

New `Submission` variant. No struct gains a field.

```rust
/// The request waits behind the stage's running or queued check of the same
/// session and tree, in the stage's waiting job, which runs once that check ends.
Waiting { pending: Box<JobRecord>, behind: String },
```

New `submit` body. It stays under 50 lines, so extract `add_request` from today's join (`submit.rs:70-74`).

```rust
let lock = store::lock_stage(work_dir, request.stage_id)?;
let Some(mut current) = live_current(&lock, work_dir, request.stage_id)? else {
    return queue(work_dir, request, now).map(Submission::Queued);
};
if covers(&current, request) {
    add_request(&mut current, request.request_id);
    store::save_current(work_dir, &current)?;
    return Ok(Submission::Joined(current));
}
if request.kind == JobKind::Check && current.kind != JobKind::Check {
    return Ok(Submission::Refused(CHECK_BEHIND_RUN.to_string()));
}
if waits_behind(&current, request) {
    return wait_behind(&lock, work_dir, &current, request, now);
}
let queued = new_record(work_dir, request, now)?;
let reason = format!("superseded by {}", queued.id);
pending::cancel_locked(&lock, work_dir, request.stage_id, &reason, now)?;
end(&mut current, &reason, now);
store::save_current(work_dir, &current)?;
persist_queued(work_dir, &queued)?;
```

New private functions in `submit.rs`:

- `live_current(lock, work_dir, stage_id) -> Result<Option<JobRecord>>`: the live current record, else `pending::promote_locked`.
- `waits_behind(current: &JobRecord, request: &SubmitRequest<'_>) -> bool`: `request.kind == Check && current.kind == Check && current.head == request.head && current.tree_digest == request.tree_digest && current.session_id == request.session_id`. Doc comment: same session only, because the job's cache policy follows its session (`child_policy::check_cache_policy`).
- `wait_behind(lock, work_dir, current, request, now) -> Result<Submission>`: `pending::live_locked`, then either `widen(&mut waiting, request)` or `new_record`. Then `store::save_pending`, then `Waiting { pending, behind: current.id.clone() }`.
- `widen(record: &mut JobRecord, request: &SubmitRequest<'_>)`: union of `criteria`, OR of `no_cache`, then `add_request`.
- `union(a: Option<&[usize]>, b: Option<&[usize]>) -> Option<Vec<usize>>`: built through `BTreeSet`, so the output is sorted and deduplicated.
- `add_request(record: &mut JobRecord, request_id: &str)`: today's lines 70-74.

Order in the supersede path: cancel the waiting job first, then the current job, then queue. A crash after the first write leaves the current job live and the waiting job cancelled. That is consistent. The opposite order could promote an old-tree waiting job over a cancelled current.

Cancel paths, in the same file:

- `cancel` (`submit.rs:118-132`):
  - The lock-free early return now also requires `store::load_pending(..)?.is_none()`.
  - Under the lock, call `pending::cancel_locked` first, then cancel current as today.
  - Return the cancelled current, else the cancelled waiting job.
  - Every caller goes through this function: `stage_takedown.rs:219-230` (`cancel_host_job`, reached by every takedown, retirement, handoff, stall requeue, park and contract hand-off), `host_jobs.rs:69-84` (`stop_running_check`), `git/cleanup/batch.rs:136`, `commands/stage/skip_retry.rs:243` and `commands/stage/merge.rs:140`. None of them changes.
- `cancel_if_id` (`submit.rs:136-151`): when the current record matches `job_id` and is live, call `pending::cancel_locked` before ending it. The waiting job was waiting behind the observed job, and the stage reset that calls this (`commands/stage/state/loop_recovery/mod.rs:103-113`) kills its session.

Match sites of `Submission` that must add the new arm (`rg -n 'Submission::' loom/src`):

| Site | New arm |
| --- | --- |
| `loom/src/orchestrator/core/inbox_drain/check.rs:60-71` `settle_submission` | `Ok(Submission::Waiting { pending, behind }) => Settle::Applied(Some(format!("host job {} queued behind {behind}", pending.id)))`. The note must start with `host job <id> ` for `wait_state::job_id_of` (`wait_state.rs:132-135`). |
| `loom/src/stage_service/contracts.rs:119-124` | add `\| Submission::Waiting { .. }` to the `Response::Ok` group. It cannot happen for a freeze; do not use `unreachable!`. |
| `loom/src/host_verify/verdict_complete.rs:195-200` | `Submission::Waiting { pending, .. } => VerdictStart::Submitted(pending.id)`. It cannot happen for a verdict completion. |

Test helpers that `match` with an `other => panic!` arm need no change.

### 2.6 Runner: the waiting job becomes current on the next tick

`loom/src/orchestrator/core/host_jobs.rs:43-63` (`tick_host_jobs`) calls `HostJobRunner::tick` (`runner.rs:98-107`), which runs `settle_held` and then `start_queued`. The pickup goes into `runner.rs::queued_records` (`runner.rs:303-315`), which both `start_queued` and `fail_queued` (the no-runner path, `host_jobs.rs:145-164`) use.

```rust
for stage_id in store::stage_ids(work_dir)? {
    if let Err(error) = super::pending::promote(work_dir, &stage_id) {
        warn_job(&stage_id, "could not promote a waiting host job", &error);
    }
    match store::load_current(work_dir, &stage_id) { /* unchanged */ }
}
```

- Delete the early return in `start_queued` (`runner.rs:199-201`). The loop's own slot check (`runner.rs:203-205`) still limits starts. Promotion must run even when every slot is busy: otherwise the current record stays terminal while a waiting job exists, and `merge.rs:127-131` (`--resolved` with a daemon) would read the stage as idle and run its gates beside a check that starts later.
- Update the doc comments of `start_queued` and `queued_records` to say that promotion happens first.
- `runner.rs` is exactly 400 lines. The edit above is net zero (+3, -3). If rustfmt or a comment pushes it past 400, move `installed_binary`, `job_deadline_secs` and `budget_for_job` (`runner.rs:317-358`) into `host_verify/runner_budget.rs`. Re-export them from `runner.rs`: `pub use super::runner_budget::job_deadline_secs;` (for `hung_latch.rs:11`) and `pub(super) use super::runner_budget::{budget_for_job, installed_binary};` (for `tests_runner_recovery.rs:12` and `verdict_complete_tests.rs:236`).
- Timing: the current check is reaped in `settle_held`, and in the same `tick` call `queued_records` promotes the waiting job. The gap in which current is terminal and a waiting job exists is microseconds, inside the daemon's single thread.
- `compare_and_set`, `settle`, `shutdown` and `fail_to_start` do not change.
- Application: `tick_host_jobs` applies the ended check after the tick (`host_jobs.rs:54-57`). Replacing `current.json` first is already normal (`apply_retry.rs:1-3`), and a check changes no stage state.

### 2.7 Restart and shutdown

- `runner.shutdown` cancels the held check with `daemon stopped` (`runner.rs:129-141`) and leaves the waiting job alone. `recover_after_restart` (`signals.rs:80-104`) interrupts a running current and leaves the waiting job alone. The first tick after the restart promotes the waiting job and starts it, just as a `Queued` current survives a restart. `signals.rs` does not change.
- The waiting job's escalation hold needs no change. `hung_latch.rs:264-287` holds a session while current is live and was relayed by that session. The waiting job shares the running job's session (row 5), so the running job holds the session until the waiting job becomes current, and then the waiting job holds it.

### 2.8 Wait side

**Keying.** `wait_state.rs::state_path` keys a check's state by its selection. Other kinds keep one file per kind: a subagent never relays `complete` or `freeze` (control kinds), and `freeze_choices_path` and `commands/stage/contracts/freeze_follow_tests.rs:60` rely on `<stage>-freeze.json`.

```rust
pub(super) fn state_path(site: &WaitSite<'_>) -> Result<PathBuf> {
    crate::validation::validate_id(site.stage_id)?;
    let name = match site.kind {
        JobKind::Check => format!("{}-check-{}.json", site.stage_id, selection_key(site)),
        kind => format!("{}-{}.json", site.stage_id, kind.name()),
    };
    Ok(site.scratch_dir.join("host-verify").join(name))
}

/// 16 hex characters naming a check's selection and cache flag; bounded for
/// any `--criterion` list, and free of `.` so `<name>.lock` stays the lock.
fn selection_key(site: &WaitSite<'_>) -> String {
    let text = format!("{:?}|{}", site.criteria, site.no_cache);
    crate::relay::sha256_hex(text.as_bytes())[..16].to_string()
}
```

- Reuse `crate::relay::sha256_hex` (`relay/ticket.rs:47-52`, re-exported at `relay/mod.rs:29`). Do not add a new hashing helper.
- `state_matches` (`wait.rs:189-194`) keeps comparing `no_cache` and `criteria`. That guards against a key collision at no cost.
- The callers that pick up the new path with no change of their own are `drive`, `pending_request_id`, `release` and `TicketSink::publish_request` (`wait.rs:98, 166, 221, 241`).

**Following a waiting job.** `wait.rs` and `wait_state::find_job` need no change.

1. The ledger note `host job <pending id> queued behind <id>` gives `applied = <pending id>` (`wait.rs:317-321`).
2. `find_job` finds the current record does not name the request. It loads the waiting job's copy (`wait_state.rs:152-158`), which is `Queued`, so the call returns `Pending { done: 0 }`.
3. After promotion, `current.requests` names the request.
4. A cancelled waiting job reads as `Finished(Cancelled)`, which exits 4 with "Run the same command again to request a new run".

**`remove_state_if_owned`.** The semantics are unchanged: delete only while the file still names `request_id`. Rewrite its doc comment (`wait_state.rs:52-56`): every call of one session for a stage and check selection shares the file. A call on a changed tree replaces the request, so one waiter's end must not discard a newer request that another waiter wrote. Rewrite the `RequestState` doc (`wait_state.rs:19`) to: "The request this session last relayed for a kind of job and, for a check, one selection."

**Stale state files.** None need cleanup code.

- A finished job's state is removed by its follower (`wait.rs:181-183`).
- A changed tree overwrites the file with the same key.
- The whole scratch directory is retired with the session (`orchestrator/core/inbox_drain/sweep.rs:46-75`).
- Old `<stage>-check.json` files are ignored. No migration: the project is unreleased.

### 2.9 Normalising `--criterion` in one place

Add this to `loom/src/commands/verify.rs`:

```rust
/// The `--criterion` list sorted and deduplicated: every consumer (the wait
/// state key and match, the relayed ticket, the job record, the local run)
/// sees one form, so a reordered or repeated list follows one request.
pub(crate) fn normalized_criteria(criteria: &[usize]) -> Vec<usize> {
    criteria.iter().copied().collect::<BTreeSet<_>>().into_iter().collect()
}
```

The first line of `execute` (`verify.rs:43`) becomes `let criteria = normalized_criteria(criteria);`. Pass `&criteria` to `session_relay::run` (`verify.rs:46`) and to `selection` (`verify.rs:76`).

Downstream consumers all receive the normalised form with no change of their own:

- `verify_relay.rs:115` (`WaitSite.criteria`), then `wait_state::state_path` and `state_matches`.
- `wait.rs:261-266` (`HostJobRequest.criteria` on the ticket).
- `inbox_drain/check.rs:52` (`SubmitRequest.criteria`), then `JobRecord.criteria`, `covers` and `union`.
- `child.rs:105` (`with_only`).

`union` keeps waiting records sorted. Do not normalise again on the daemon side: `covers` uses `contains` and does not depend on order.

### 2.10 Known limits (record in knowledge, section 8)

- **A waiting job's verdict covers its whole selection.** If reviewer B on {2} and reviewer C on {3} wait together, both see the {2, 3} verdict. Joining a subset into a full job works the same way today (`tests_submit.rs:196-205`). Revisit if reviewers misread failures of criteria they did not select.
- **Two followers of one selection share one state file.** The first follower to see the job end removes the state (`wait.rs:181-183`). The second follower's next call relays a new request. On an unchanged tree its passes hit the cache, and a failing criterion runs again. Revisit if `full_runs.py` field data shows repeated failing runs in integration-verify.

## 3. Struct-literal sites

No field is added to `JobRecord`, `SubmitRequest`, `RequestState` or `HostJobRequest`. None of the 66 `JobRecord {` sites, 24 `SubmitRequest {` sites or 2 `RequestState {` sites changes, including `tests/*/support.rs`. Only the enum `Submission` gains a variant; its three exhaustive matches are listed in 2.5.

## 4. Current file sizes (limit 400)

| File | Lines | Change |
| --- | --- | --- |
| `loom/src/host_verify/runner.rs` | 400 | net zero; fallback move in 2.6 |
| `loom/src/host_verify/store.rs` | 394 | about -9 (`save_current` delegates), +2 (`mod`, `use`) |
| `loom/src/host_verify/submit.rs` | 228 | about +45 |
| `loom/src/host_verify/wait_state.rs` | 180 | about +12 |
| `loom/src/host_verify/mod.rs` | 215 | +5 (`pending`, two test mods) |
| `loom/src/host_verify/tests_wait.rs` | 389 | +4 (`state_file_for`, `pub(super) fn ledger`) |
| `loom/src/host_verify/tests_submit.rs` | 246 | three tests rewritten, `waiting` helper, helpers `pub(super)` |
| `loom/src/host_verify/tests_wait_criteria.rs` | 97 | about +90 |
| `loom/src/host_verify/wait_state_tests.rs` | 89 | about +35 |
| `loom/src/host_verify/tests_child_reuse.rs` | 156 | about +25 |
| `loom/src/commands/verify.rs` | 268 | about +12 |
| `loom/src/commands/hook/relay/tests.rs` | 368 | about +14 |
| `loom/src/orchestrator/core/inbox_drain/check.rs` | 349 | about +20 |
| `loom/src/orchestrator/core/event_handler/host_job_cancel_tests.rs` | 83 | about +25 |
| `loom/src/stage_service/contracts.rs`, `host_verify/verdict_complete.rs` | - | one arm each |
| `loom/src/host_verify/tests_store.rs` (395), `tests_runner.rs` (391) | - | add nothing here |

New files: `host_verify/store/slots.rs`, `host_verify/pending.rs`, `host_verify/tests_submit_pending.rs`, `host_verify/tests_runner_pending.rs`. Declare the test files in `host_verify/mod.rs` as `#[cfg(test)] mod tests_submit_pending;` and `#[cfg(test)] mod tests_runner_pending;`, matching `mod.rs:40-83`.

## 5. Tests

Each test must fail against today's tree for the reason given.

### 5.1 `tests_submit.rs` (existing, rewritten because the behaviour changes)

- Make `STAGE`, `HEAD_A`, `HEAD_B`, `DIGEST`, `state_root`, `request`, `subset`, `queued` and `joined` `pub(super)`. Add `pub(super) fn waiting(submission: Submission) -> (JobRecord, String)`, which panics on any other variant.
- `an_uncached_request_supersedes_a_cached_job` (line 117) becomes `an_uncached_request_waits_behind_a_cached_job`. Assert `Waiting`, `pending.no_cache`, `behind == cached.id`, and that `load_current` is still `cached` (Queued, `detail == None`).
- `a_full_request_does_not_join_a_subset_job` (line 207) becomes `a_full_request_waits_behind_a_subset_job`. Assert the waiting job's `criteria == None` and that the current job is not cancelled.
- `equal_subsets_join_and_a_wider_subset_supersedes` (line 220) becomes `equal_subsets_join_and_a_wider_subset_waits`.

### 5.2 `tests_submit_pending.rs` (new; uses the 5.1 helpers)

| Test | Asserts |
| --- | --- |
| `two_checks_on_other_criteria_of_one_tree_never_cancel_each_other` | Plan acceptance. r1 {1} is queued and r2 {2} is `Waiting` behind it. Current is r1's job, still `Queued`, `detail None`. `load_pending` has `criteria == Some([2])` (criterion 1 is not in it) and `requests == ["r2"]`. |
| `waiting_selections_merge_into_one_waiting_job` | r2 {2}, r3 {3} and r4 {2} return one waiting id with `criteria [2, 3]` and requests `[r2, r3, r4]`. Then r5 (full) gives `criteria None`, and r6 with `no_cache` gives `no_cache true`. |
| `a_request_the_running_job_covers_still_joins_it` | r1 full, then r2 {1} returns `Joined(r1 job)`, and `load_pending` is `None`. |
| `a_new_tree_supersedes_the_running_check_and_its_waiting_one` | Plan acceptance. r1 {1}, r2 {2} waits, r3 at `HEAD_B` returns `Superseded`. The r1 job and the waiting copy (`load_job_record`) are both `Cancelled` with `superseded by <new id>`. `load_pending` is `None` and current is the new job. |
| `another_session_on_the_same_tree_still_supersedes` | r1 {1} from `session-a`, then {2} from `session-b`, returns `Superseded`. No waiting job. |
| `a_completion_supersedes_a_check_and_its_waiting_check` | r1 {1}, r2 {2} waits, then a `Complete` request on the same tree returns `Superseded`. The waiting copy is `Cancelled`. |
| `a_check_never_waits_behind_a_completion` | A queued `Complete` (cached), then a `no_cache` check, returns `Refused(CHECK_BEHIND_RUN)`. No waiting job. |
| `cancel_ends_the_waiting_check_with_the_current_one` | `cancel(.., "stage agents taken down")` ends both with that detail, and `load_pending` is `None`. |
| `cancel_ends_a_waiting_check_whose_current_job_already_ended` | Current saved `Passed`, waiting job saved with `save_pending`. `cancel` returns the cancelled waiting record. |
| `cancel_if_id_ends_the_waiting_check_behind_the_observed_job` | With the matching id, both are cancelled. With another id, nothing changes. |
| `a_submit_after_the_current_job_ended_promotes_the_waiting_check_first` | Current `Passed` plus a waiting {2}. Submitting r9 {2} returns `Joined(waiting id)`. Current is that job, `Queued`, and `load_pending` is `None`. |
| `a_waiting_slot_whose_copy_was_cancelled_is_dropped` | `save_pending` (Queued), then `save_job_copy` (Cancelled), then `pending::promote` returns `None`. `pending.json` is gone and current is unchanged. |
| `a_promotion_cut_short_after_saving_current_only_clears_the_slot` | `save_current(waiting)` with `pending.json` left in place. `pending::promote` returns that job and clears the slot. |

Inline tests in `store/slots.rs` (`#[cfg(test)] mod tests`, as `store/order.rs:43` does):

- `a_waiting_record_is_saved_beside_current_with_its_job_copy`
- `the_job_copy_wins_over_a_stale_pending_file`
- `a_symlinked_pending_file_is_refused` (as `tests_store.rs` does for `current.json`)
- `clearing_an_absent_pending_file_succeeds`

### 5.3 `tests_runner_pending.rs` (new; `/bin/sh` runner from `tests_runner.rs`)

Gate the child on a release file so the first job is still `Running` when the second request arrives:

```rust
let script = format!(r#"while [ ! -e "$LOOM_WORK_DIR/release" ]; do sleep 0.05; done; {PASS}"#);
let mut runner = HostJobRunner::new(work.clone(), shell(&script, 2));
```

| Test | Asserts |
| --- | --- |
| `a_waiting_check_runs_after_the_running_check_ends` | Plan acceptance, end to end. Start r1 {1} and `tick_until(running)`. r2 {2} returns `Waiting`. Touch `release`, then `tick_until(id == waiting id && Passed)`. The r1 job's record is `Passed` (never `Cancelled`). The waiting job ran with `criteria Some([2])`. `load_pending` is `None`. |
| `a_waiting_check_becomes_current_while_every_slot_is_busy` | `slots = 1` and a second stage `stage-b` (`write_stage`). stage-a's r1 runs, stage-b's job is queued, stage-a's r2 waits. Release, then tick until stage-b runs. stage-a's current is the waiting job, `Queued`. This guards the `merge --resolved` gap in 2.6. |
| `a_waiting_check_survives_a_restart` | Current saved `Running` (with `started_at`) plus a waiting job. `recover_after_restart` leaves current `Interrupted`. A fresh runner's tick runs the waiting job to `Passed`. |
| `with_no_runner_a_waiting_check_fails_with_the_reason` | Current saved `Passed` plus a waiting job. `fail_queued(&work, "no verify-host runner: x")` returns the waiting job as `Failed` with that detail. |

These tests are not `#[serial]`: like `tests_runner.rs` they set no process environment.

### 5.4 `tests_wait_criteria.rs` (existing file, new tests)

- Make `Fx::ledger` (`tests_wait.rs:108`) `pub(super)`.
- Add `pub(super) fn state_file_for(&self, no_cache: bool) -> PathBuf { state_path(&self.site(Duration::ZERO, no_cache)).unwrap() }` to `Fx`, and make `state_file()` return `self.state_file_for(false)` (`tests_wait.rs:83-85`).
- Update `tests_wait.rs:350` and `:362` (`a_changed_no_cache_or_tree_starts_a_new_request`) to read `state_file_for(true)`.
- Update `tests_wait_criteria.rs:96`: the r2 subset {0, 2} now lives in its own file, so read `state_path` of a site with `criteria: Some(&[0, 2])`.

| Test | Asserts |
| --- | --- |
| `two_selections_of_one_session_keep_two_requests` | Plan acceptance. A call with {1} relays r1 and a call with {2} relays r2. Save current with criteria [1] and requests [r1]; no job names r2 yet (inside `REQUEST_GRACE`). A second {1} call returns `Pending` with no ticket, and a second {2} call returns `Pending` with no ticket. Both state files exist, holding r1 and r2. Today the second {1} call relays again. |
| `reordered_and_repeated_criteria_follow_one_request` | Plan acceptance. `call(&normalized_criteria(&[2, 1]), "r1")` returns `Requested`. Save current with criteria [1, 2] and requests [r1]. `call(&normalized_criteria(&[1, 2, 1]), "r2")` returns `Pending` with no ticket. |
| `a_request_waiting_behind_another_job_follows_the_waiting_record` | A {2} call relays r2. Current A has criteria [1] and requests [r1], `Running`. Waiting job B is saved with `store::save_pending` (requests [r2]). `fx.ledger("r2", Applied, "host job <B> queued behind <A>")`. A call returns `Pending { done: 0 }` with no ticket. Then `save_job_copy(B as Cancelled)`, and a call returns `Finished(Cancelled)`, `exit_code == 4`, with the state file removed. |

### 5.5 `wait_state_tests.rs`

- `each_check_selection_has_its_own_state_file`: the sites (None, false), (Some([1]), false), (Some([2]), false) and (None, true) give four distinct paths under `host-verify/`. Each name matches `^s1-check-[0-9a-f]{16}\.json$`.
- `a_completion_or_freeze_keeps_one_state_file_per_kind`: `Complete` with `no_cache` true or false is `s1-complete.json`, and `Freeze` is `s1-freeze.json`.

### 5.6 `commands/verify.rs` tests

- `criterion_selections_are_sorted_and_deduplicated`: `[2, 1]` gives `[1, 2]`, `[1, 2, 1]` gives `[1, 2]`, and `[]` gives `[]`.

### 5.7 `inbox_drain/check.rs` tests

- `a_check_waiting_behind_another_settles_with_the_waiting_job_id`: two `submit`s in a temp state root give `Waiting`. `settle_submission(Ok(waiting))` is `Settle::Applied(Some(note))`, where `note == format!("host job {} queued behind {}", pending.id, behind)`.

### 5.8 `event_handler/host_job_cancel_tests.rs`

- `taking_down_a_stages_agents_cancels_its_waiting_check_too`: `queue_check` (full), then a `no_cache` check on the same tree returns `Waiting`. `take_down_agents` leaves current `Cancelled` and the waiting copy `Cancelled`, both with `stage agents taken down`, and `load_pending` is `None`.

### 5.9 `commands/hook/relay/tests.rs`

- `a_subagent_check_request_is_relayed_and_attributed_to_the_subagent`: modelled on `a_subagent_memory_request_is_attributed_to_the_subagent` (line 207). Use `write_ticket(.., RequestKind::Check)`, a payload with `Some("general-purpose")` and allowed `[RequestKind::Check]`. The reply contains `received check`, and the stored `InboxEntry` has `kind == Check` and `agent == AgentRole::Subagent`. If `write_ticket` builds a memory-only payload, give it a `HostJobRequest` JSON payload: `{"head": 40 hex, "tree_digest": 64 hex, "no_cache": false, "criteria": [0]}`.

### 5.10 `tests_child_reuse.rs` (serial, real confinement)

- `a_waiting_selection_reuses_the_pass_the_running_check_stored`: `#[serial]`, with `confinement_unavailable` skip. `Fixture::new(&["true", "true"], Standard)`. A job with criteria [0] passes. A job with criteria [0, 1] reports `acceptance-0` with `(cached)` and `log None`, and `acceptance-1` ran (`log Some`). This proves the plan's claim that the criteria the first job passed hit the cache.

## 6. Ordered implementation steps

Each step ends with the narrow command named. Run each one once.

1. **Store slots.** Add `store/slots.rs` (2.3), make `save_current` delegate, and add the inline tests.
   `cargo test --manifest-path loom/Cargo.toml --lib host_verify::store::`
2. **Waiting-job lifecycle.** Add `host_verify/pending.rs` (2.4), declare it in `mod.rs`, and make `submit::end` `pub(super)`.
   `cargo build --manifest-path loom/Cargo.toml`
3. **Submission rules and cancel paths.** Change `submit.rs` (2.5), add the `Submission::Waiting` arms at the three match sites, rewrite the three `tests_submit.rs` tests and add `tests_submit_pending.rs`.
   `cargo test --manifest-path loom/Cargo.toml --lib host_verify::tests_submit`
   This filter also runs `tests_submit_pending`.
4. **Runner pickup.** Change `queued_records` and `start_queued` (2.6), then `wc -l loom/src/host_verify/runner.rs`. Do the fallback move if it is above 400. Add `tests_runner_pending.rs`.
   `cargo test --manifest-path loom/Cargo.toml --lib host_verify::tests_runner_pending::`
5. **Wait-state keying.** Change `state_path` and add `selection_key` (2.8), update the doc comments, update `Fx` in `tests_wait.rs` and the two tests named in 5.4, and add the 5.5 tests.
   `cargo test --manifest-path loom/Cargo.toml --lib host_verify::tests_wait`
   This prefix matches `tests_wait`, `tests_wait_criteria`, `tests_wait_refusal` and `tests_wait_freeze`. Cargo takes one filter, so the 5.5 tests in `host_verify::wait_state::tests::` run in the same `cargo test` only if the engineer runs that filter instead; prefer `host_verify::tests_wait`, because it covers the fixture change.
6. **Normalisation.** Add `normalized_criteria` and call it in `execute` (2.9), add the 5.6 test, then the 5.4 tests that use it.
   `cargo test --manifest-path loom/Cargo.toml --lib host_verify::tests_wait_criteria::`
7. **Drain note, takedown, relay and cache tests** (5.7 to 5.10). Pick the narrowest filter, for example:
   `cargo test --manifest-path loom/Cargo.toml --lib commands::hook::relay::tests::a_subagent_check`
8. **Knowledge** (section 8).

The final gate belongs to the main agent and is not run by the implementer:

```bash
cargo test --manifest-path loom/Cargo.toml
cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path loom/Cargo.toml --check
cargo test --manifest-path loom/Cargo.toml --test maintainability
```

## 7. Traps

- **Size limit.** `runner.rs` is at the 400-line limit, and `tests_store.rs` (395), `tests_runner.rs` (391), `tests_wait.rs` (389) and `store.rs` (394) are close. The limit is enforced by `loom/tests/maintainability/scanner.rs:6` (`FILE_LINE_LIMIT`). Functions must stay at or under 50 lines: extract `add_request`, `widen` and `union` rather than growing `submit`.
- **Serial tests.** Real-confinement child tests must be `#[serial]` with the `confinement_unavailable` skip (`tests_child.rs:24`), because the spawn observer is a single process-wide slot. The `/bin/sh` runner tests need no `serial`. Any test that sets `LOOM_ACCEPTANCE_CACHE` or `PATH` must be `#[serial]`; none of the tests above should.
- **Lock discipline.** Never call `store::lock_stage` while holding it. `promote` takes the lock; `promote_locked`, `cancel_locked` and `live_locked` require it, which the `&StageLock` parameter enforces. `cancel`'s lock-free pre-check must not create the stage directory: `load_current` and `load_pending` both read without creating anything (`store_fs.rs:121-142`).
- **Write order.** Write the job copy before the slot when saving. When cancelling, cancel the waiting job before `current`. These orders are what make crashes safe (2.2, 2.4, 2.5); do not reorder them.
- **No compatibility code.** Do not add migration or compatibility code: no reading of old `<stage>-check.json`, and no `serde(default)` additions. `JobRecord` keeps `deny_unknown_fields` and gains no field.
- **No `unreachable!`.** Do not use `unreachable!` for `Submission::Waiting` in `contracts.rs` or `verdict_complete.rs`.
- **Note format.** The settle note must keep `host job <id> ` as its prefix, or `job_id_of` cannot find the waiting job.

## 8. Knowledge updates (after the code)

`doc/loom/knowledge/architecture/verification-v2-gates.md`. Use `loom knowledge replace-section` and state the current truth only.

- `## Job Records and State`: add `pending.json`, the stage's waiting check. Describe its job copy, the rule from table 2.1 (same session and tree, any live current check, union of waiting selections only), and the write order.
- `## Components`: add `host_verify/pending.rs` (promote and cancel of the waiting job) and `store/slots.rs`. Say that `submit.rs` queues, joins, waits or supersedes. Say that a check's wait state is keyed by its selection and cache flag (`wait_state.rs::state_path`).
- `## Cancellation and Restart`: every cancel path also cancels the waiting job. Superseding a check cancels its waiting job. The runner's tick promotes a waiting job once the current job ends, even while every slot is busy. A restart leaves the waiting job for the first tick.
- The integration-verify text the plan asks for (section 3, last line) is written by the worker that owns the doctrine change, not by this one.

`doc/loom/knowledge/concerns/verification-v2-followups.md`:

- Delete the `verify_relay.rs::request_or_wait` bullet at line 153. Normalisation now happens in `commands/verify.rs::normalized_criteria`.
- Under `## Verification v2 Known Gaps` (line 31), add the two known limits from 2.10, each with its revisit condition.

Finish with `loom knowledge check`.
