# verification-evidence / w3-host-job-records: stranded jobs settle, damaged history refuses reuse, gates hold at the transition

Tier: opus
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/verification.md`, sections E3, E4, CR-37.

## Findings covered

- E3: a job CAS'd to `Running` whose spawn failed, and whose `Failed` write then failed too, stays `Running` with no child and no holder forever, so the stage's pending job is never promoted.
- E4: `latest_of` skips an unreadable or mismatched job record, so an older passing `Complete` can outrank a newer failure and be reused for a verdict completion.
- CR-37 (narrowed): review rounds and `integrity.json` change under the stage's reviews-directory lock, which the completion transition does not hold, so a round with open findings recorded after the outside gate check is missed.

## Files

Own (write):
- `loom/src/host_verify/runner.rs`
- `loom/src/host_verify/runner_records.rs` (new)
- `loom/src/host_verify/apply.rs`
- `loom/src/host_verify/apply_binding.rs` (new)
- `loom/src/host_verify/verdict_complete.rs`
- `loom/src/host_verify/verdict_reuse.rs`
- `loom/src/host_verify/mod.rs`
- `loom/src/host_verify/tests_runner_pending.rs`
- `loom/src/host_verify/verdict_reuse_tests.rs`
- `loom/src/host_verify/tests_complete_support.rs`
- `loom/src/host_verify/tests_apply_gate_lock.rs` (new)
- `loom/src/verify/review/verdict_records.rs` (outside the stage territory; the plan author confirms it; one helper)

Read only: `loom/src/host_verify/store.rs`, `loom/src/host_verify/pending.rs`, `loom/src/host_verify/signals.rs`, `loom/src/host_verify/gates.rs`, `loom/src/daemon/server/observer.rs` (`check_completion_gates`), `loom/src/verify/review/store.rs`, `loom/src/verify/review/gate.rs`, `loom/src/fs/locking.rs`, `loom/src/verify/transitions/persistence.rs`, `loom/src/commands/hook/review_harvest.rs`, `loom/src/orchestrator/core/host_jobs.rs`.

Frozen (contract tests, never edit): `loom/src/host_verify/runner_orphan_contract_tests.rs`, `loom/src/host_verify/verdict_history_contract_tests.rs`.

## Changes

### 1. E3: fail `Running` records no runner holds (`runner.rs`, new `runner_records.rs`)

`runner.rs` is at 400 lines. Move `enum Cas`, `fn compare_and_set` and `fn end_record` (lines 70-75, 360-387) unchanged into `loom/src/host_verify/runner_records.rs` as `pub(super)` items, declare `mod runner_records;` in `host_verify/mod.rs`, and import them in `runner.rs`. Add to `runner_records.rs`:

```rust
/// What a job that never got a child records.
pub(super) const NEVER_STARTED: &str = "verify-host never started";

/// Fail every stage's current record that is `Running` with no `pid` and is
/// not `held`: the runner writes `Running` only for a job it is about to
/// spawn, holds it from the spawn on, and a previous daemon's `Running`
/// record is interrupted by startup recovery before the first tick. Returns
/// the records it failed; a failure on one stage is logged and skipped.
pub(super) fn fail_unheld_running(work_dir: &Path, held: &dyn Fn(&str) -> bool) -> Result<Vec<JobRecord>>
```

For each `store::stage_ids(work_dir)?`: `store::load_current`; when the record is `JobState::Running`, `pid.is_none()` and `!held(&record.id)`, run `compare_and_set(work_dir, stage, id, JobState::Running, |r| end_record(r, JobState::Failed, Some(NEVER_STARTED.to_string())))` and push `Cas::Applied` records. Log per-stage errors with the same `tracing::warn!` shape as `warn_job`.

In `HostJobRunner::tick` (line 98), after `settle_held` and before `start_queued`, call it with `&|id| self.running.contains_key(id)` and extend `ended` (log an error, as the `start_queued` error is logged). It must run before `start_queued`, whose `queued_records` promotes the pending job once the current one is terminal (`pending::promote_locked`). Do not touch `fail_to_start`; the scan is what settles the case where its CAS failed.

### 2. E4: `latest_of` fails closed and cross-checks the current slot (`verdict_reuse.rs`)

New signature (pinned):

```rust
pub(super) fn latest_of(
    work_dir: &Path,
    stage_id: &str,
    kinds: &[JobKind],
) -> Result<std::result::Result<Option<JobRecord>, String>>
```

- Per job directory: `Ok(Some(record))` whose id and stage match is a candidate when its kind is in `kinds`; `Ok(Some(_))` with another identity returns `Ok(Err(format!("the stage's host job history cannot be ordered: job {job_id} does not match its directory")))`; `Err(error)` returns `Ok(Err(format!("the stage's host job history cannot be ordered: job {job_id} is unreadable ({error:#})")))`; `Ok(None)` is skipped. Delete `readable_record`.
- Then `store::load_current`: an `Err` returns `Ok(Err(..."the current job record is unreadable"...))`. A current record whose kind is in `kinds` wins when no candidate exists, when its id has no per-job record among the candidates read, or when no candidate outranks it by `(submission_order, queued_at)` (it ties with its own per-job copy, and then the current slot is the one taken).
- Keep each function at 50 lines or fewer (extract the ranking into a helper).
- `decides` (line 25): `Ok(Err(_))` from `latest_of` gives `Ok(false)`, so a fresh run is queued.
- Move `counted_result` and `run_counts` (`verdict_complete.rs` lines 115-164) unchanged into `verdict_reuse.rs` as `pub(super)`, so `verdict_complete.rs` (395 lines) has room; update its imports.
- `verdict_complete.rs` line 90: `Ok(Err(reason))` maps to `VerdictStart::Ineligible(reason)`. `latest_completion_head` (line 237, test-only) becomes `Ok(latest_of(..)?.map_err(anyhow::Error::msg)?.map(|record| record.head))`.
- Update the module docs of both files where they describe `latest_of` ("A record that cannot be read is skipped" is now false).

### 3. CR-37: re-run the gates under the reviews lock at the transition

- `loom/src/verify/review/verdict_records.rs`: add (pinned)

```rust
/// Run `f` holding the stage's `reviews/` directory lock, the lock every
/// writer of a review round or verdict record takes.
pub(crate) fn with_reviews_locked<T>(work_dir: &Path, stage_id: &str, f: impl FnOnce() -> Result<T>) -> Result<T>
```

  built from `canonical_work_dir` + `store::stage_dir` + `locked_dir_update`, and make `update` call it.
- New `loom/src/host_verify/apply_binding.rs` (declare `mod apply_binding;` in `mod.rs`): move `BindingFailed`, its `Display`/`Error` impls, `binding_failed`, `binding_reason` and `verify_bound_run` out of `apply.rs` (lines 61-77, 235-242, 310-340) unchanged; `apply.rs` and `verdict_complete.rs` import them from there. Add:

```rust
/// Run `complete` while the stage's review records cannot change, once the
/// completion gates pass on them. Lock order: the caller's sessions lock or
/// dispute claim, then this reviews lock, then the stage file `complete`
/// takes. The gates take seconds, held under those locks once per completion.
pub(super) fn with_gates_held<T>(work_dir: &Path, stage_id: &str, complete: impl FnOnce() -> Result<T>) -> Result<T>
```

  Body: `with_reviews_locked(work_dir, stage_id, || { if let Some(reason) = gates::refusal(work_dir, stage_id) { return Err(binding_failed(reason)); } complete() })`.
- `apply.rs`, `complete_stage` (line 274): inside `locked_dir_update(&sessions_dir, ..)`, after `validate_active_identity`, wrap the `update_stage(..)` call in `with_gates_held(work_dir, &record.stage_id, || update_stage(..))`. The gates must run outside `update_stage`: `check_completion_gates` calls `load_stage`, whose `locked_read` takes a second descriptor on the stages directory that `update_stage` holds exclusively. Keep `check_outside_lock` as it is: it refuses the common case without taking locks.
- `verdict_complete.rs`, `complete_held` (line 300): inside the block holding `_claim`, wrap `update_stage(..)` the same way. This covers `apply_passed` and `verdict_reuse::complete_from_run`.
- Test hook, in `apply_binding.rs`, `#[cfg(test)] pub(crate) mod hooks` with a `thread_local! RefCell<Option<Box<dyn FnOnce()>>>`: `pub(crate) fn before_completion_lock(hook: impl FnOnce() + 'static)` arms it, `pub(super) fn run_before_completion_lock()` takes and runs it. Call `#[cfg(test)] super::apply_binding::hooks::run_before_completion_lock();` in `apply.rs` `apply_passed` between `check_outside_lock` and `complete_stage`, and in `verdict_complete.rs` `apply_passed` between `gates::refusal` and `complete_held`.

### 4. Lock-order proof (put it in your report)

Show, with the `rg` commands you ran, that no path takes the reviews lock and then the sessions lock, a dispute claim or the stages lock: `rg -n 'locked_dir_update|locked_read|lock_exclusive|lock_shared|lock_stage_dispute_dir|update_stage|save_stage' loom/src/verify/review loom/src/verify/integrity loom/src/verify/impact_tests loom/src/commands/hook/review_harvest.rs`, then read every closure passed to `locked_dir_update` on a reviews directory (`store::write_round`, `store::record_round` and its `decide` closure in `review_harvest.rs` including `journal_suggestions`, `verdict_records::update` and its `build` closures, `impact_tests::observed`). Also show the gates (`check_completion_gates` and what it calls) take no lock on `reviews/<stage>` or `sessions/`, which this process then already holds. If any path takes them in the other order, stop and report it instead of changing it.

## Interfaces shared with other workers

None. `with_reviews_locked`, `with_gates_held`, `fail_unheld_running`, `NEVER_STARTED` and the new `latest_of` are used only by the files above.

## Tests

- E3, `loom/src/host_verify/tests_runner_pending.rs`, `a_running_job_without_a_child_is_failed_and_the_waiting_check_runs`: `state_root()`; `first = queued(submit(&work, &subset("r1", &[1]), now))`; set `state = Running`, `started_at = Some(now)`, `pid = None`, `store::save_current`; `(next, _) = waiting(submit(&work, &subset("r2", &[2]), now))`; `HostJobRunner::new(work.clone(), shell(PASS, 1))`; one `tick()` returns `first` as `Failed` with detail `verify-host never started`; then `tick_until(.., |r| r.id == next.id && r.state == JobState::Passed)`. Fails today: `first` stays `Running`.
- E4, `loom/src/host_verify/verdict_reuse_tests.rs` (fixtures `held()`, `submit()`, `current()`): `an_unreadable_newer_completion_refuses_reuse`: `fx.passed()`, then `newer = fx.finished(JobKind::Complete, JobState::Failed, &[("acceptance-0", false)])`, overwrite `store::job_dir(&fx.work_dir, STAGE, &newer.id)?.join("record.json")` with `b"not json"`; `submit` is `VerdictStart::Ineligible(reason)` with `reason` containing `cannot be ordered`; stage still `NeedsAdjudication`. And `a_current_completion_without_its_job_directory_outranks_the_pass`: same, but `std::fs::remove_dir_all` the newer job's directory; `submit` is not `Reused`. Both fail today (`Reused`).
- CR-37, new `loom/src/host_verify/tests_apply_gate_lock.rs` (declare `#[cfg(test)] mod tests_apply_gate_lock;`): in `tests_complete_support.rs` turn the body of `record_review` into `pub(crate) fn record_review_round(work_dir: &Path, stale: bool, open_findings: usize)` (`record_review` calls it with 0; each finding is a `RecordedFinding { id: finding_id(round, k), finding: Finding { severity: "high", file: "src/lib.rs", line: 1, claim: "an open finding", scenario: None, rule: None } }`). Test `a_finding_recorded_after_the_outside_gate_check_rejects_the_completion`: `Fixture::new(StageType::Standard, 2)`, `fx.record_review(false)`, `record = fx.passed()`, arm `hooks::before_completion_lock` with a closure that calls `record_review_round(&work_dir, false, 1)`; `fx.apply(&record)` is `Applied::Recorded` and `fx.assert_rejected(&record)`. Test `a_finding_recorded_before_a_verdict_transition_hands_the_stage_back`: held stage (as `verdict_complete_tests::held(StageType::Standard, 2)` does) with `fx.record_review(false)`, a passed `VerdictComplete` record bound to `STAGE_BOUND_SESSION`, the same hook; `apply::apply_finished` is `Applied::HandedBack(_)` and the stage is not `Completed`. Both fail today (`Completed`). Mark them `#[serial_test::serial]`.
- Contracts this worker turns green: `childless-running-job-is-settled`, `unreadable-newer-completion-refuses-reuse`.

## Proof

From `loom/`:

```bash
cargo check --all-targets
cargo test --lib host_verify::
cargo test --lib verify::review::
cargo test --lib orchestrator::core::host_jobs
cargo test --test host_verification_contracts
```

## Traps

- The scan must skip a job this runner holds whose pid write failed (`start`, line 247): it is `Running` with `pid: None` and is held.
- Failing every pid-less `Running` record in `queued_records` would fail a job between the Queued-to-Running CAS and `self.running.insert` only if the scan ran inside `start`; keep it a separate step in `tick`.
- Do not re-run the gates inside `verify_bound_run` or any `update_stage` closure (self-deadlock on the stages directory lock).
- Do not drop `check_outside_lock`'s gate call.
- `latest_of` must refuse on any unreadable record, whatever its kind: its kind is unknown.
- `verdict_complete.rs` and `apply.rs` must end under 400 lines after `cargo fmt`; check with `wc -l`.
