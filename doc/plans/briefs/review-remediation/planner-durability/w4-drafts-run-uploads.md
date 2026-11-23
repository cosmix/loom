# planner-durability / w4-drafts-run-uploads: continuation target in one write, run admission under the job guard, upload store bounds, guarded batch hooks

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/planner-backend.md`, sections P6, P7, CR-61 (sub-claims 61a and 61b only) and CR-76 (sub-claim 76g only).

You run no `git` command, touch no path under `.loom/`, and run no tests. Your proof is `cargo check --all-targets` from `loom/`. The orchestrator runs the tests you write. Look bodies up yourself with `loom map --outline <file>`, `loom map --find-all <symbol>` and `rg`.

## Findings covered
- P6: `continue_draft` creates the new draft without its target and writes the target in a second `drafts::update`, whose failure is only logged; the old draft is then removed, losing the confirmed target.
- P7: `accept_start` reads the run state and admits before it takes the run job guard, so a job that runs in between is invisible to the decision.
- CR-61(a): identical bytes uploaded under a second media type add a second blob (`<id>.<ext2>`) beside the first and overwrite the one sidecar; the first blob is orphaned and still counts against the cap.
- CR-61(b): the expiry sweep runs only at server start, so a long-lived server refuses uploads as full while expired files remain.
- CR-76(g): a panicking batch hook unwinds through `apply_batch` and kills the `loom-plan-watch` thread silently.

## Files
Own (write), paths under `loom/`:
- `src/plan_server/chat/drafts.rs`, `src/plan_server/chat/draft_continue.rs`, `src/plan_server/chat/draft_cmd.rs`, `src/plan_server/chat/tests_drafts.rs`, `src/plan_server/chat/tests_dispatch/draft_commands.rs`
- `src/plan_server/run/start.rs`, `src/plan_server/run/start/tests_refusal.rs`
- `src/plan_server/uploads/store.rs`, `src/plan_server/uploads/tests_store.rs`
- `src/plan_server/watch.rs`

Read only: `src/plan_server/chat/kickoff_cmd.rs` (`record_owner`, the other `on_confirm` caller), `src/plan_server/run/child.rs` (`RunJobs::begin`, `JobGuard`), `src/plan_server/run/state.rs` (`state_view`), `src/plan_server/run/commit/tests_route.rs` (how a route test builds an `ApiRequest`), `src/plan_session/attachments/stored.rs` (`locate`, `read_sidecar`), `src/plan_server/context.rs` (`on_batch`, `batch_hooks`).

Frozen (contract test, never edit): `src/plan_server/chat/draft_seed_contract_tests.rs`.

Another worker (w1) edits `src/plan_server/chat/mod.rs` and `src/plan_server/chat/dispatch.rs`; never edit them. `src/plan_server/chat/tests.rs` (the chat fixture) is read only for both of you.

## Changes

### 1. P6: the continuation's draft carries its target from its first write
- `plan_server/chat/drafts.rs` struct `Seed` (near line 90): add `pub(super) target: Option<String>` (one doc line: the confirmed document a continuation resumes). `Draft::started` sets `target: seed.target` instead of `None`. The file is 392 lines: add nothing else to it.
- Every `Seed { .. }` literal gains the field: `draft_cmd.rs` `StartDraft::of` (`target: None`), `draft_continue.rs` `continue_draft` (`target: target.map(str::to_owned)`), `tests_drafts.rs` `seed()` and the two other literals (`target: None`).
- `draft_continue.rs` `continue_draft`: keep `record_owner(bridge, &id, grant.size, &grant.name)` when both `grant` and `target` are `Some`; delete the `on_confirm(ctx, state, &id, target)` call and drop `on_confirm` from the `use` line (it stays in `draft_cmd.rs` for `kickoff_cmd.rs`). The old draft is still removed only after `drafts::create` returned `Ok`, and a failed create still ends the new session and refuses (unchanged).
- Not in this unit: surfacing failed draft writes of a start or a send to the browser (the validation marks it optional; no browser slot exists).

### 2. P7: `plan_server/run/start.rs` `accept_start`
Order becomes: read-only refusal; `req.json()`; `child::jobs(req.ctx).begin(RunJob::Start)` (refusal `409 busy` "another run job is in progress", unchanged text); `state_view`; `admit`; the context upgrade; `current_exe`; `spawn_start(ctx, guard, ..)`. Every early return after `begin` drops the guard (`JobGuard::drop`), so a refused start leaves no job active. `admit` does not read `state.busy`, so holding the guard during the read changes no admission result. Passing the plan to `loom run` is not part of this fix.

### 3. CR-61(a): one blob per id (`plan_server/uploads/store.rs` `put_within`)
A delivered attachment path must not go away (existing test `the_same_bytes_under_another_media_type_keep_the_first_file` pins that). So, under the store lock, before the `present` check: `stored::locate(&dir, &id)`; when it returns a data file whose sidecar `media_type` differs from the request's, publish no new blob: rewrite the sidecar with the new `name` and `stored_at` and the stored `media_type`, and return `FileMeta` with the stored `media_type`. Same media type: unchanged path. Doc the function: the first media type of a content id wins.

### 4. CR-61(b): sweep on full
Split `sweep(dir, now)` into the public locking wrapper and `fn sweep_locked(dir: &Path, dirfd: &OwnedFd, now: u64) -> Swept` (the body without `lock()`). In `put_within`, when the cap check fails: run `sweep_locked(&dir, &dirfd, now)` (the store lock is held: `STORE_LOCK` is not re-entrant, so calling `sweep` there deadlocks), measure `stored_bytes` again, and return `StoreError::Full` only if it still does not fit. `uploads/mod.rs` `install` keeps calling `sweep` once at start.

### 5. CR-76(g): `plan_server/watch.rs` `apply_batch`
Wrap each `hook(ctx, batch)` in `std::panic::catch_unwind(AssertUnwindSafe(..))`; on `Err` log `tracing::error!("a planning batch hook panicked")` and go on with the next hook and the checklist broadcast. Same pattern as `plan_session/bridge/mod.rs` `hear`.

## Interfaces shared with other workers
None at the symbol level. w1 adds a field to `ChatState` in `chat/mod.rs`; you construct `ChatState::default()` in tests as today, which keeps compiling.

## Tests (write them; the orchestrator runs them)
- P6, `chat/tests_drafts.rs`: `a_seeded_target_is_stored_by_create` (`drafts::create` with `Seed { target: Some("doc/plans/PLAN-demo.md".into()), ..seed() }`, then `drafts::get(..).unwrap().target` is that path; fails to compile today). `chat/tests_dispatch/draft_commands.rs`, helpers `fixture`, `write_draft`, `continue_cmd`, `draft_file`, `stored`, `plan_draft` (near line 23): `a_continued_confirmed_draft_keeps_its_target` (continue a draft whose target document exists, the new draft's stored `target` equals it, the old file is gone).
- P7, `run/start/tests_refusal.rs`, building the request as `run/commit/tests_route.rs` does and the context as `run/state/tests.rs` does (`child::install_jobs`, `start::install`) over a `crate::test_git::TestRepo` with a modified tracked file outside `doc/`: `a_start_while_a_job_runs_is_busy_before_any_state_read` (hold `jobs(&ctx).begin(RunJob::Commit)`; the start returns 409 `busy`; today 409 `blocked`) and `a_refused_start_leaves_no_job_active` (no job held; 409 `blocked`; then `jobs(&ctx).current()` is `None`).
- CR-61, `uploads/tests_store.rs`, helpers `runtime`, `store_dir`, `stored`, `id_of`: `a_second_media_type_adds_no_blob` (put `text/plain`, then `text/markdown`, same bytes: exactly one data file `<id>.txt`, the second result's `media_type` is `text/plain`; fails today with two blobs); `a_full_store_sweeps_expired_files_before_refusing` (put A at `now = 1` with a large cap; measure the store with A in it; put B (different bytes, no longer than A) at `now = 1 + KEEP_SECONDS + 10` with `cap` set to that measured size, so B fits only once A's data file and sidecar are gone: B is stored and A's files are gone; fails today with `Full`).
- CR-76(g), `watch.rs` `tests`, helper `context(root)`: register a panicking hook then a recording hook with `ctx.on_batch`, call `apply_batch` with a non-initial `Batch`: the recording hook ran and the call returned.
- Contract to turn green: `continuation-target-written-later`.

## Proof
`cargo check --all-targets` from `loom/`. The orchestrator then runs `cargo test --lib plan_server::chat::`, `cargo test --lib plan_server::run::start::`, `cargo test --lib plan_server::uploads::` and `cargo test --lib plan_server::watch::`.

## Traps
- Adding `Seed.target` but leaving `Draft::started` at `target: None`: the contract fails.
- Removing the first blob for a second media type breaks a path already pasted into a session.
- Calling `sweep` (which locks) from `put_within` (which holds the lock) hangs.
- Moving `begin` after `json()` is fine; moving it before the read-only refusal is not needed.
- `drafts.rs` must stay under 400 lines after `cargo fmt`.
