# runtime-and-dashboard-server / w1-worker-journal: journal reserve and torn-tail repair

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/runtime.md`, sections R6 and R7.

## Findings covered
- R6 (narrowed): a full `events.jsonl` makes `Terminal` and the ownership record fail to append, so the unit stays `Ownership::Held` and blocks stage operations. `Progress` is not the consumer; `ServerRequestRefused { method }` (method up to 1 MiB) is.
- R7: a torn final line in `events.jsonl` makes `read_events` fail, so `runner_recover::pass_stage` skips the unit and never settles it.

## Files
Own (write), all under `loom/src/native_worker/`:
- `ids.rs`, `model.rs`, `model_tests.rs`, `store.rs`, `store_tests.rs`, `machine/mod.rs`, `index.rs` (one token only), `runner_recover.rs`, `runner_recover_tests.rs`
- `store_repair.rs` (new), `store_repair_tests.rs` (new), `runner_repair.rs` (new), `runner_repair_tests.rs` (new)
Read only: `store_io.rs`, `supervisor/teardown.rs`, `index_tests_damage.rs`.
Frozen (contract tests, never edit): `store_contract_tests.rs`, `runner_recover_contract_tests.rs` (written before you start; they must turn green).

## Changes

### 1. R6: reserve space for settlement records
1. `ids.rs`, next to `MAX_RECORD_BYTES` (line ~16): add `pub const SETTLEMENT_RESERVE_BYTES: u64 = 16_384;` and `pub const SETTLEMENT_TEXT_BYTES: usize = 2_048;`, each with a one-line doc. Add both names to the `pub use super::ids::{..}` list at the top of `model.rs`.
2. `model.rs`: there is no `impl NativeEvent`. Add one after the enum (variants at lines ~87-130):
   - `pub fn is_settlement(&self) -> bool`: true for `Terminal`, `OwnershipReleased`, `OwnershipUnverified`.
   - `pub fn clamp_settlement_text(self) -> Self`: for those three variants, truncate `reason`, `by` and `detail` to at most `SETTLEMENT_TEXT_BYTES`, cut at `str::floor_char_boundary`; every other variant returns unchanged.
3. `store.rs`, `EventLog::append` (line ~252): first `let event = event.clamp_settlement_text();`. Compute `let cap = if event.is_settlement() { MAX_RECORD_BYTES } else { MAX_RECORD_BYTES - SETTLEMENT_RESERVE_BYTES };` and test `self.bytes + line.len() as u64 > cap` (today it compares to `MAX_RECORD_BYTES`). Keep `StoreError::TooLarge("events.jsonl")`, the no-poison and no-sequence-consumption behaviour (the test `event_log_size_limit_does_not_poison_or_consume_sequence` must still pass unchanged). Readers keep `MAX_RECORD_BYTES`: a file filled to the cap must still read.
4. `machine/mod.rs`, `Machine::on_frame`, the `Incoming::ServerRequest` arm (line ~193): the reply keeps using the full `method` (`policy::reply_for(&method)`); only the persisted event carries `method` cut to at most 256 bytes at a char boundary. Do this with a small private fn in `machine/mod.rs` (`fn bounded_method(method: String) -> String`).
5. Callers of `append` (`supervisor/teardown.rs::settle`, `runner_recover.rs::settle_lost_supervisor`, `runner.rs`) need no change.

### 2. R7: repair a torn tail of a record whose supervisor is gone
1. `store.rs`: split the line loop of `read_events` (lines ~213-227) into `pub(super) fn parse_events(text: &str) -> Result<Vec<EventRecord>, StoreError>` (same `Corrupt("line N: ...")` messages and contiguous-`seq` check); `read_events` keeps the UTF-8 check and the unterminated-last-line refusal, then calls `parse_events`. Behaviour of `read_events` does not change.
2. `store.rs`: declare the repair module as a child so it can reach the test helpers: add `#[path = "store_repair.rs"] mod repair;` and `pub use repair::repair_torn_tail;` above the `#[cfg(test)]` tests module declaration.
3. New `store_repair.rs`: `pub fn repair_torn_tail(dir: &Path) -> Result<bool, StoreError>`:
   - `read_regular(dir, "events.jsonl", MAX_RECORD_BYTES)` (from `super::super::store_io`); `None` or empty or ending in `\n` or invalid UTF-8: `Ok(false)`.
   - Let `cut` be the index just past the last `\n`. No `\n` at all: `Ok(false)` (nothing valid to keep; the unit stays corrupt and is reported).
   - `parse_events(&text[..cut])?`: any complete line that fails to parse or breaks the `seq` run is a middle corruption: return that `Err` and change nothing.
   - Write the dropped bytes (`bytes[cut..]`) to `events.jsonl.torn-<unix seconds>` with `safe_create_new_in_workdir(fd.as_raw_fd(), Path::new(&name), tail)` on `safe_open_dirfd(dir)` (as `mark_spawn_begun` does in `store.rs`). An existing sidecar of that name is an error returned to the caller (no overwrite); the next pass retries.
   - Truncate: `let file = open_append(dir)?; file.set_len(cut as u64)?; file.sync_all()?;` (`set_len` works on the `O_WRONLY|O_APPEND` handle). Return `Ok(true)`.
   - Decision: always preserve and truncate; never try to complete an unterminated record that looks valid. `settle_lost_supervisor` re-derives a lost `Terminal` as `Outcome::Unknown`.
4. `index.rs`: change `fn list_invocations` (line ~158) to `pub(super) fn list_invocations`. Nothing else in that file (398 lines; it must not grow).
5. New `runner_repair.rs`, declared in `runner_recover.rs` (this file is loaded as module `recover` through `#[path]` from `runner.rs`, so state the path): add `#[path = "runner_repair.rs"] mod repair;` near the top of `runner_recover.rs`. Contents:
   `pub(super) fn repair_lost_tails(work_dir: &Path, stage_id: &str, found: &mut Vec<Warning>) -> bool` (returns whether any record was repaired). For each `Ok(entry)` of `index::list_invocations(&store::stage_dir(work_dir, stage_id))`, with `dir = entry.path()`:
   - call `store::repair_torn_tail` only when the claim proves the supervisor gone, so first `read_claim(&dir)` (private in the parent module, reachable from this child through `super::read_claim`; the same holds for `supervisor_gone`, `tombstone`, `CLAIM_GRACE`, `Warning`, `WarnKind`, `Notes`):
     - `Ok(Some(claim))` with `claim.pid <= 0`: gone.
     - `Ok(Some(claim))` otherwise: gone only if `supervisor_gone(&dir, &claim, &mut notes)` (same proof `decide` uses); copy `notes` into `found` as warnings the way `pass_stage` does.
     - `Ok(None)`: gone only when `store::read_manifest(&dir)` is `Ok`, its `created_at` is older than `CLAIM_GRACE`, and `tombstone(&dir)` returned true (the same step `decide` takes for an old unclaimed record).
     - `Err(_)`: skip (`decide` reports an unreadable claim).
   - When gone, call `store::repair_torn_tail(&dir)`: `Ok(true)` logs `tracing::warn!` naming the invocation and the sidecar and sets the return flag; `Ok(false)` nothing; `Err(error)` pushes `Warning::new(stage_id, Some(invocation_id), WarnKind::CannotSettle, format!("{error:#}"))` (a corrupt middle line surfaces this way and nothing is truncated).
   Tests of this file go in `runner_repair_tests.rs`, declared at the bottom of `runner_repair.rs` as `#[cfg(test)] #[path = "runner_repair_tests.rs"] mod tests;`.
6. `runner_recover.rs`, `pass_stage` (lines ~228-272, 45 lines today; keep it at most 50, extract the `corrupt` warning tail into a helper if needed): load `let mut units = index::stage_units(..)`; when any element is `Err` and `repair::repair_lost_tails(pass.work_dir, stage_id, found)` returns true, load the units again. Healthy stages therefore pay nothing. Then run the existing loop over `units`.

## Interfaces shared with other workers
None. `NativeEvent::is_settlement`, `clamp_settlement_text`, `SETTLEMENT_RESERVE_BYTES`, `SETTLEMENT_TEXT_BYTES` and `store::repair_torn_tail` are new public names used only inside this worker's files and by the frozen contract tests.

## Tests
Fixtures: `store_tests.rs` has `invocation() -> (TempDir, PathBuf)` (line ~26) and `manifest(..)`; `runner_recover_tests.rs` has `record(work) -> PathBuf`, `tail(dir)`, `mark_spawn_begun` import, `Pass`/`run_pass` via `use super::*`.
1. `store_tests.rs` (335 lines today; keep it under 400, add at most about 40 lines): 
   - `settlement_records_use_the_reserve`: `log.bytes = MAX_RECORD_BYTES - SETTLEMENT_RESERVE_BYTES`; `append(Progress)` is `Err(StoreError::TooLarge(_))`; `append(Terminal { outcome: Outcome::Unknown, reason: "x".repeat(10_000) })` and `append(OwnershipReleased { by: "daemon".into() })` both `Ok`; the stored `Terminal` reason is at most `SETTLEMENT_TEXT_BYTES` bytes; `read_events` still parses the file. Fails today (`TooLarge`).
   - Make `invocation` `pub(super)` so `store_repair_tests.rs` can reach it through `super::super::tests::invocation`.
2. `store_repair_tests.rs`: `torn_tail_is_preserved_and_truncated` (append raw `{"seq":2` without newline to `events.jsonl`; `repair_torn_tail` is `Ok(true)`; `read_events` is `Ok` with one record; a sidecar named `events.jsonl.torn-*` holds exactly `{"seq":2`); `repair_refuses_a_corrupt_middle_line` (replace the file by `broken\n` + a valid line + a partial tail: `Err(Corrupt(s))` with `s` starting `line 1:`, file bytes unchanged, no sidecar); `repair_is_a_no_op_on_a_clean_or_newline_less_file` (clean file: `Ok(false)`; file `{"seq":1` with no newline anywhere: `Ok(false)`, unchanged).
3. `runner_repair_tests.rs` (use `record()` from `runner_recover_tests.rs`; make it `pub(super)` there): `a_torn_tail_of_a_gone_supervisor_is_repaired_and_settled` (torn tail plus `supervisor.json` naming a dead pid, or `tombstone(&dir)`; `run_pass` returns `Step::Settled`; `read_events` ok; sidecar present); `a_torn_tail_under_a_live_supervisor_is_left_alone` (write `supervisor.json` as `{"pid":<this test process id>,"start":"<crate::process::process_start_time of that pid>"}`; after `run_pass` the file bytes are unchanged, no sidecar exists, and the pass returned a `Step::Warn` for the corrupt record); `a_corrupt_middle_line_is_refused_not_truncated` (`Step::Warn`, bytes unchanged, no sidecar).
4. `runner_recover_tests.rs` (291 lines): `a_full_journal_is_still_settled`: fill a record with `ServerRequestRefused` events through `EventLog::open(&dir)` in descending chunk sizes (1 MiB, 64 KiB, 4 KiB, 200 bytes) until each size returns `TooLarge`; `settle_lost_supervisor(&dir)` is `Ok`; `tail(&dir)` is `Terminal` then `OwnershipReleased`. Fails today.
5. `machine/tests.rs` is not yours; the existing test at `machine/tests.rs:229` keeps a short method and still passes. Add one assertion-bearing test to `model_tests.rs` for `clamp_settlement_text` (multi-byte text cut on a char boundary) and `is_settlement`.
Contract tests to turn green: `native_worker::store_contract_tests::*` and `native_worker::runner_recover_contract_tests::*`.

## Proof
From `loom/`: `cargo check --lib`, `cargo test --lib native_worker::store`, `cargo test --lib native_worker::model`, `cargo test --lib native_worker::runner`, `cargo test --lib native_worker::machine`, `cargo test --lib native_worker::index`.

## Traps
- Raising `MAX_RECORD_BYTES` for settlement events above what `read_regular` accepts would make the file unreadable; the reserve is carved out of the same 4 MiB.
- Truncating `reason` after serialising the line does not shrink the line; clamp before `serde_json::to_string`.
- `repair_torn_tail` must never run while the supervisor may still append: gate it on the claim proof exactly as above; do not call it from `EventLog::open`.
- `run_pass` is also the daemon-start path (`recover_after_restart`): the repair runs inside `pass_stage`; a separate scheduler tick would leave the unit unsettled after the single recovery pass.
- `index.rs` is at 398 lines: change only the visibility token.
- A test that builds a claim for the test process must use a real start token, otherwise `verify_process_identity` reads it as `Dead`.
