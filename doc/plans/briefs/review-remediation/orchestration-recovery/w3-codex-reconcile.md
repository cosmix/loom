# orchestration-recovery / w3-codex-reconcile: bounded codex reconcile scans visit every active job

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md`, section R5.

You run no `git` command, touch no path under `.loom/`, and run no tests. Your only check is `cargo check --all-targets` from `loom/`. The orchestrator runs the tests you write.

## Findings covered

- R5: `active_stage_sessions` caps the session list at 256 before dropping terminal records, so historical records crowd out live ones; and `reconcile_at` always takes the first 64 stages of a sorted map, so a 65th active stage is never reconciled and its Direct codex job never reaches the lifecycle index.

## Files

Own (write):
- `loom/src/codex_lifecycle/reconcile.rs`
- `loom/src/codex_lifecycle.rs`
- `loom/src/orchestrator/monitor/core.rs`
- `loom/src/codex_lifecycle/reconcile_window_tests.rs` (new)

Read only: `loom/src/codex_lifecycle/test_support.rs`, `loom/src/codex_lifecycle/ledger.rs`, `loom/src/codex_lifecycle/direct_tests.rs`, `loom/src/codex_lifecycle/tests.rs`, `loom/src/commands/subagents/wait/direct_tests.rs`.

Frozen (contract tests, never edit): `loom/src/codex_lifecycle/reconcile_window_contract_tests.rs`.

## Changes

Look up each symbol with `loom map --find-all <symbol>` or `rg` before editing.

1. `active_stage_sessions` (`codex_lifecycle/reconcile.rs`): apply `.filter(|session| !session.status.is_terminal())` before `.take(MAX_ACTIVE_SESSIONS)`. Nothing else in that function changes.

2. Rotation in `codex_lifecycle/reconcile.rs`:
   - Add `pub(crate) fn reconcile_from(work_dir: &Path, active_sessions: &[Session], state_root: &Path, now: DateTime<Utc>, cursor: usize) -> Result<ReconcileReport>`. It holds today's body of `reconcile_at`, except the stage loop: collect `active_stage_sessions(..)` into a `Vec<(String, HashSet<String>)>` (sorted, as the `BTreeMap` yields it), let `n = len`; when `n == 0` return the empty report with `next_cursor = 0`; otherwise `start = cursor % n`, visit `take = n.min(MAX_ACTIVE_STAGES)` stages at indices `(start + i) % n` for `i in 0..take`, and set `report.next_cursor = (start + take) % n`. The per-stage body (ledger read, `StageReceipts::load`, `reconcile_stage_rows`) is unchanged; move it into a helper so every function stays under 50 lines.
   - `reconcile_at` keeps its exact signature and becomes `reconcile_from(work_dir, active_sessions, state_root, now, 0)`. Its callers in `commands/subagents/wait/direct_tests.rs` and `codex_lifecycle/test_support.rs` stay untouched.
   - `reconcile_with_state_root` keeps its signature (cursor 0).
   - Every eligible stage is then visited within `ceil(n / 64)` polls while the active set is stable.

3. `codex_lifecycle.rs`:
   - `ReconcileReport` (line 195): add `pub next_cursor: usize` with a doc line (where the next poll's stage window starts). It derives `Default`; no other construction site exists.
   - `reconcile_codex_jobs` (line 261): new signature `pub fn reconcile_codex_jobs(work_dir: &Path, active_sessions: &[Session], cursor: usize) -> Result<ReconcileReport>`; it calls `reconcile::reconcile_from(work_dir, active_sessions, &state_root, Utc::now(), cursor)`.
   - Next to `pub(crate) use reconcile::reconcile_at;` (line 330) re-export `reconcile_from` the same way.
   - Declare the new test file beside the existing test modules: `#[cfg(test)] mod reconcile_window_tests;`.

4. `orchestrator/monitor/core.rs`:
   - Add a private field `codex_cursor: usize` to `Monitor` (line 73) and set it to `0` wherever `Monitor` is constructed (find it with `rg -n "Monitor \{" loom/src/orchestrator/monitor`).
   - `reconcile_codex_evidence` (line 41) gains `cursor: &mut usize`; it passes `*cursor` to `reconcile_codex_jobs` and, on `Ok(report)`, sets `*cursor = report.next_cursor` before walking the entries. On `Err` the cursor stays.
   - `Monitor::poll` (line 151) passes `&mut self.codex_cursor`.

## Interfaces shared with other workers

- None with other workers of this stage.
- Pinned for the frozen contract file: `crate::codex_lifecycle::reconcile_from(work_dir: &Path, active_sessions: &[Session], state_root: &Path, now: DateTime<Utc>, cursor: usize) -> Result<ReconcileReport>` and `ReconcileReport::next_cursor: usize`.

## Tests

Write these in `codex_lifecycle/reconcile_window_tests.rs`, using `super::test_support::{liquidmon, at, only_outcome, LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED}` and `crate::subagent_lifecycle::WorkerOutcome`:

- `terminal_records_do_not_crowd_out_an_active_session`: `let (fixture, _) = liquidmon(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;` build a session list of 256 `Session` values with `status = SessionStatus::Completed`, ids `old-000`..`old-255` and stage ids of the same names, followed by `fixture.session.clone()`; call `super::reconcile_at(&fixture.work_dir, &sessions, &fixture.state_root, at(3600))`; assert `only_outcome(&report)? == &WorkerOutcome::Succeeded`. Today the report is empty.
- `the_stage_window_rotates_past_the_first_sixty_four`: a `tempfile::tempdir()` with `work/` and `state/` created; for `i in 0..65` a stage id `s{i:03}`, a session `sess-{i:03}` (`Running`, `stage_id` set) and the ledger `work/subagents/<stage>/codex.jsonl` holding `not json\n` (each visited stage then yields one `Unknown` entry naming it). The first `reconcile_from(.., cursor 0)` reports 64 stages and `next_cursor == 64`; the second, from that cursor, reports `s064`. Today `reconcile_from` does not exist.
- `an_empty_active_set_resets_the_cursor`: `reconcile_from(.., &[], .., 7)` returns no entries and `next_cursor == 0`.

Contract to turn green: `codex-reconcile-visits-every-stage` (file `loom/src/codex_lifecycle/reconcile_window_contract_tests.rs`).

## Proof

`cargo check --all-targets` from `loom/`. Then report the files you changed; the orchestrator runs `cargo test --lib codex_lifecycle::` and `cargo test --lib orchestrator::monitor::`.

## Traps

- Filtering terminal sessions after the 256 cap is the bug; the filter must come first.
- Changing `reconcile_at`'s signature breaks five call sites in `commands/subagents/wait/direct_tests.rs`, a file outside your ownership; add `reconcile_from` instead.
- A cursor kept in the report but never stored on the monitor rotates nothing: `Monitor::poll` must feed it back on the next poll.
- `cursor % n` with `n == 0` panics; return early.
