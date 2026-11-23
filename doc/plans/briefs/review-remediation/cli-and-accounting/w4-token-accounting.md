# cli-and-accounting / w4-token-accounting: rewrite detection, shed restart, final-capture completeness

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, sections D6, D7, D8. Where this brief differs from those sections, this brief wins (the differences are listed in Traps).

## Findings covered
- D6: a transcript rewritten in place between two scans is not noticed unless it shrank below the saved offset or got a new inode. A same-length rewrite adds nothing; a longer one is read from the middle.
- D7: when the checkpoint is over its size cap, `clear_tables_except` empties the message-id tables of idle agents. A later revised line for an id the table forgot is added in full instead of replacing the old observation, so it is counted twice.
- D8: a final token capture that failed or missed a file still lets `finish_plan` archive the totals as the permanent plan record.

## Files
Own (write):
- `loom/src/tokens/tracker.rs`
- `loom/src/tokens/source_read.rs` (new)
- `loom/src/tokens/anchor.rs` (new)
- `loom/src/tokens/accumulate.rs`
- `loom/src/tokens/checkpoint.rs`
- `loom/src/tokens/tail.rs`
- `loom/src/tokens/mod.rs`
- `loom/src/tokens/planner.rs` (one import line)
- `loom/src/tokens/tracker_tests.rs` (one import line and a `mod` line at the end)
- `loom/src/tokens/tracker_rewrite_tests.rs` (new)
- `loom/src/tokens/checkpoint_tests.rs`
- `loom/src/tokens/tail_tests.rs`
- `loom/src/daemon/server/orchestrator.rs`
- `loom/src/daemon/server/orchestrator_tests.rs`

Read only: `loom/src/tokens/pass_memo.rs`, `loom/src/tokens/plan_totals.rs`, `loom/src/tokens/messages.rs`, `loom/src/tokens/tests_fixture.rs`, `loom/src/plan_stats/finish.rs`, `loom/src/utils.rs` (`sha256_hex`).
Frozen (never edit): `loom/src/tokens/rewrite_contract_tests.rs`, `loom/src/tokens/shed_contract_tests.rs`, `loom/src/daemon/server/final_capture_contract_tests.rs` (new, written before your work).

## Changes

### 0. Make room in `tracker.rs` first
`tracker.rs` is 366 lines. Move `read_sources`, `read_source` and `count_last_line` (the block from `/// Reads every source until` through `count_last_line`) into the new `source_read.rs`, unchanged, then edit them there. `LastLine` stays in `tracker.rs`; `source_read.rs` imports it with `use super::tracker::LastLine;`. `tracker.rs` calls `super::source_read::read_sources`. Update the two other importers: `planner.rs` (`use super::tracker::{.., read_sources, ..}` becomes an import from `super::source_read`) and `tracker_tests.rs` (its `use super::{..., read_sources, ...}`). Add `mod anchor;` and `mod source_read;` to `tokens/mod.rs`. Remove imports in `tracker.rs` that nothing uses any more.

### 1. D6: content anchor (new `anchor.rs`, `tail.rs`, `accumulate.rs`, `source_read.rs`, `checkpoint.rs`)
- `tail.rs`: add `pub(crate) fn read_range(path: &Path, start: u64, end: u64) -> io::Result<Option<Vec<u8>>>`: opens with the same `O_NOFOLLOW | O_NONBLOCK` flags as `read_fragment`, seeks to `start`, reads `end - start` bytes; `Ok(None)` when the file holds fewer than `end` bytes; `Ok(Some(vec![]))` when `end <= start`.
- `anchor.rs`: `pub(super) const ANCHOR_BYTES: u64 = 4096;` `pub(super) fn of_prefix(path: &Path, end: u64) -> io::Result<Option<u64>>`: the fingerprint of the bytes `[0, end)` of the file. When `end <= 2 * ANCHOR_BYTES` hash the whole range; otherwise hash the first `ANCHOR_BYTES` followed by the last `ANCHOR_BYTES` bytes before `end`. Hash with `crate::utils::sha256_hex` and take the first 16 hex digits as a `u64` (`u64::from_str_radix`). Do NOT use `pass_memo::new_hasher`: it is keyed randomly per process and the anchor is saved in the checkpoint and compared by a later process. `end == 0` returns `Ok(Some(0))`; `None` when the file is shorter than `end`. `pub(super) fn rewritten(path: &Path, cursor: &FileCursor) -> io::Result<bool>`: `false` when `cursor.offset == 0`, else `of_prefix(path, cursor.offset)? != Some(cursor.anchor)`.
- `accumulate.rs`, `FileCursor`: add `pub(super) anchor: u64` (fingerprint of the file prefix ending at `offset`, 0 while `offset` is 0) with a doc line. Derive stays (`Default`, `Serialize`, `Deserialize`); no `serde(default)`.
- `checkpoint.rs`: `CHECKPOINT_VERSION` 4 becomes 5. No migration: an older checkpoint already falls to `load_reporting`'s "checkpoint is version N" warning and is rebuilt from the transcripts.
- `source_read.rs`, `read_source`: replace the restart test with
  ```text
  moved = file.path != source.path || file.inode != stat.inode || stat.len < file.offset
  shed_and_grown = file.table_shed && stat.len > file.offset      (see D7)
  if moved || shed_and_grown || anchor::rewritten(path, file)? { agent.restart(&source.path, stat.inode) }
  ```
  evaluated in that order (the `?` propagates an I/O error as a read failure, like the later read). It runs before the `stat.len == agent.file.offset` early return, so a same-length rewrite is caught. After the read (including when the read returned an error part way, and after `count_last_line`), set `agent.file.anchor = anchor::of_prefix(path, agent.file.offset).ok().flatten().unwrap_or(0);` so the saved anchor matches the saved offset; 0 on a failure makes the next pass restart, which is safe. Keep the order: assign the offset, compute the anchor, then return the read error.
  Update the doc comment of `read_source` to list the rewrite and shed restarts.

### 2. D7: mark shed tables (`accumulate.rs`, `checkpoint.rs`, `source_read.rs`)
- `FileCursor`: add `pub(super) table_shed: bool` ("the message-id table was emptied to fit the checkpoint cap; the next advance rebuilds the agent from offset 0").
- `Checkpoint::clear_tables_except` (`checkpoint.rs`): for an agent not in `keep` whose `file.messages.order` is non-empty, replace the table with the default and set `agent.file.table_shed = true`. Leave an agent whose table is already empty alone (a Codex agent never has one; marking it would force a pointless restart). Rewrite the doc comment: totals stay exact until the file next grows, then the agent is rebuilt.
- `read_source`: the `shed_and_grown` condition above. `AgentState::restart` already rebuilds `FileCursor` from `Default`, which clears `table_shed` and `anchor`.
- `pass_memo.rs` needs no change (`save_shedding` calls `clear_tables_except` on the in-memory checkpoint, so the flag is also in the saved JSON).

### 3. D8: completeness of the final capture (`tracker.rs`, `mod.rs`, `orchestrator.rs`)
- `tracker.rs`: add
  ```rust
  /// How a final scan ended.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum FinalScan {
      /// Every transcript was read to its end.
      Complete,
      /// A transcript could not be read, so the totals miss it; the text names the first one.
      Incomplete(String),
  }
  ```
  `Pass` gains `incomplete: Option<String>`. In `run_pass` take `let incomplete = read_failure.clone();` before `problems.extend(read_failure);` and put it in the returned `Pass`. `final_scan(work_root, codex_sessions_root, now) -> Result<FinalScan>`: after the existing `log_change(&mut None, pass.warning)`, return `FinalScan::Incomplete(reason)` when `pass.incomplete` is `Some`, else `FinalScan::Complete`. `scan_pass` is unchanged. Export `FinalScan` from `tokens/mod.rs` next to `final_scan`. Callers that ignore the value (`tracker_handle_tests.rs`, `plan_totals_tests.rs`, `plan_totals_lanes_tests.rs` call `.expect("final scan")`) keep compiling.
- `orchestrator.rs`, `publish_plan_stats`: `match crate::tokens::final_scan(work_root, None, now)`: `Ok(FinalScan::Complete)` continues to `finish_plan`; `Ok(FinalScan::Incomplete(reason))` logs `tracing::error!("final token scan could not read every transcript ({reason}); plan statistics not published")` and returns; `Err(error)` logs `tracing::error!("final token scan failed: {error:#}; plan statistics not published")` and returns. The outcome stays `Success` (the stages are done). Nothing is archived on those paths, so `finish_plan`'s "first archive wins" rule does not pin partial totals and a later successful capture can still publish. No repair trigger is added.
- The process-global registration map (`plan_totals::register`, keyed by work root): read in the code, not a live defect. One daemon process runs one plan (`supervise_worker` sets the shutdown flag after the run; `finish_foreground` is one process per run), and publication is now skipped whenever the final scan fails before registering. Do not add a `clear`.

## Interfaces shared with other workers
None. `FinalScan` and `final_scan` are used only by `orchestrator.rs` (yours).

## Tests
Shared fixtures: `loom/src/tokens/tests_fixture.rs` (`Fixture::new`, `add_session`, `add_subagent`, `scan`, `tokens_dir`, `body`, `append`, `claude_line`, `usage`, `bulky_body`, `stage`, `unix_now`, `SESSION_UUID`). `tracker_tests.rs` is 391 lines and must not grow beyond the import edit and the two-line `#[path = "tracker_rewrite_tests.rs"] mod rewrite;` declaration at its end; the new tests live in `tracker_rewrite_tests.rs` (`use super::*;` reaches `line`, `alpha_total`, `pass_at` of the parent).
- `a_same_length_rewrite_is_counted_from_the_new_content`: lines `line("m1", now - 90, 100)`, `line("m2", now - 80, 100)`; scan; `fs::write` (same inode) with `line("m1", now - 90, 200)`, `line("m2", now - 80, 300)` (assert the byte lengths are equal first); scan; `alpha_total(..).cache_write == 500`. Fails today (stays 200).
- `a_longer_rewrite_between_scans_matches_a_fresh_scan`: overwrite with different, longer content; the total equals a scan of a second `Fixture` holding the final bytes.
- `appended_lines_are_added_without_a_restart`: scan, append one line, scan: total is the sum and the saved cursor offset equals the file length (`Checkpoint::load(&fx.tokens_dir())`).
- `a_revised_message_after_shedding_is_not_counted_twice`: reproduce `over_cap_checkpoint_empties_the_tables_of_files_that_did_not_advance` (main bulky body, pass, add subagent bulky body, pass; assert `saved_table_len(&fx, &main_key) == 0`), then append to the main transcript one line for the already-counted id `format!("main{0:0>1200}", 0)` with a timestamp one second newer than the one `bulky_body` wrote and a different `cache_write`; run `pass_at`; compare `stage(&flow, "alpha").total` with a scan of a second `Fixture` holding the same final bytes of both files. Fails today (double count). Also assert the saved checkpoint marks the main agent `table_shed` after the shedding pass.
- `checkpoint_tests.rs`: `clear_tables_except_marks_only_non_empty_tables_shed` (an agent with a filled table is marked, one with an empty table is not, a kept one is untouched).
- `tail_tests.rs`: `read_range_returns_the_bytes_or_none_for_a_short_file`.
- `orchestrator_tests.rs` (helpers `merged_run`, `redirect_home`, `transcript_line`, `archived_stats` are in that file): 
  - `a_failed_final_scan_publishes_no_plan_statistics`: set up as `published_plan_statistics_count_a_line_written_after_the_last_checkpoint` does (session with a transcript, `scan_pass` so `tokens/checkpoint.json` exists), then rename `tokens` aside and put a symlink named `tokens` pointing at it (`ensure_dir` refuses it). `finish_orchestration` returns `Success`, `plan-stats.json` is absent and `archived_stats(&home) == 0`. Then restore the directory and call `finish_orchestration` again: `plan-stats.json` exists and `archived_stats(&home) == 1`. Fails today (the stale totals are archived by the first call).
  - `an_unreadable_transcript_withholds_the_archive_until_it_can_be_read`: return early when `libc::geteuid() == 0`; same setup, then append one more line to the transcript (so the file has bytes to read) and `chmod 000` it; `finish_orchestration` gives `Success` with no stats archived; restore mode 0644 and a second call archives one record.
- Contracts to turn green: `tokens::rewrite_contract_tests::a_same_length_rewrite_is_counted_from_the_new_content`, `tokens::shed_contract_tests::a_revised_message_after_shedding_is_counted_once`, `daemon::server::final_capture_contract_tests::a_failed_final_scan_publishes_no_plan_statistics`.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib tokens::`; `cargo test --lib daemon::server::orchestrator`; `cargo test --lib daemon::server::final_capture_contract_tests`.

## Traps
- The validation section suggests the pass memo already skips unchanged transcripts. It does not: `read_sources` stats and now also reads up to 8 KiB of every source with a saved offset on every pass (5 s). That is accepted cost; do not add a per-file memo.
- The validation section says to publish after a partial read failure. The assignment reverses that: an `Incomplete` capture publishes nothing.
- The anchor window is 4096 bytes, not 256: transcript lines are kilobytes, and a field changed near the start of the last line must still change the fingerprint. A rewrite that changes only bytes outside the first and last windows is not detected; that limit is deliberate.
- The anchor is computed after the read from the file again; a rewrite landing in that gap is missed. Accepted.
- `restart` clears `table_shed` and `anchor` through `FileCursor::default()`; do not copy them across.
- `clear_tables_except` runs on the memo's in-memory checkpoint too; the flag must be set there, not only in the serialised copy.
- Keep `tracker.rs` under 340 lines after the move; `tracker_tests.rs` stays under 395.
