# token-core W2: tracker cadence and pass memo

Tier: opus (`loom-senior-software-engineer`, effort high). Read `doc/plans/briefs/live-token-insights/common.md` first; section 4 is your spec. Paths are relative to `loom/`.

You own: `src/tokens/mod.rs`, `src/tokens/tracker.rs`, `src/tokens/tracker_tests.rs`, new `src/tokens/pass_memo.rs` and `src/tokens/pass_memo_tests.rs`, `src/tokens/quota_points.rs`, `src/tokens/quota_points_tests.rs`. W1 changes the wire types, `accumulate.rs`, `checkpoint.rs` and `flow_build.rs` at the same time: do not edit those, and expect their signatures to stay as they are today except `CHECKPOINT_VERSION` (2). Never touch `tests/token_insights_contracts.rs` or `tests/token_insights_contracts/` (frozen contracts).

## Today (read these first)

- `tracker.rs:90-110` `scan_until`: `ensure_tokens_dir`, `Checkpoint::load_reporting` (reads and parses the whole `checkpoint.json`, up to 8 MiB, every pass), `discover`, `read_sources`, `prune`, `build_flow`, `quota_left(work_root, now)` (reads both quota history files, up to 16 MiB each, every pass: `quota_points.rs:23,55`), `save_checkpoint` (always writes, with a directory fsync in `quota::cache::atomic_write`), `write_flow` (always writes).
- `tracker.rs:30-46` `TrackerLocal` already carries per-thread state (`stop`, `warning`); `run_tracker` sets `stop` on the `tokens-tracker` thread. Tests call `scan_pass` on their own threads.
- `TRACKER_INTERVAL_SECS` (`tracker.rs:25`) is used only at `:244` and re-exported in `mod.rs`; no test pins 30.

## Steps

1. `TRACKER_INTERVAL_SECS` = 5. Update the doc comments that say 30 s (`spawn_token_tracker`, the module doc if it says so).
2. New module `pass_memo.rs` (declare `mod pass_memo;` in `mod.rs`) holding `FLOW_HEARTBEAT_SECS: i64 = 60`, `QUOTA_LEFT_REFRESH_SECS: i64 = 30`, a `FileStamp { inode: u64, len: u64, mtime_nanos: i128 }` taken with `std::fs::symlink_metadata` (`None` when the file is missing), and `PassMemo` with the fields of common.md section 4, keyed by `work_root` (store the `PathBuf` as given; a different path means a fresh memo). Hash JSON with `std::collections::hash_map::DefaultHasher` (stable within one process, which is all the memo needs). `TrackerLocal` gains `memo: Option<PassMemo>`.
3. Rewire `scan_until` so that, in order: the checkpoint comes from the memo when `checkpoint.json`'s stamp equals the memo's, else from `load_reporting`; the save is skipped when the compact JSON hash equals the memo's; an `OverCap` result or an error clears the checkpoint part of the memo; after a real save the memo records the new stamp and hash; `quota_left` is reused per section 4 (have `quota_points.rs` return, or expose, the paths it read so their stamps can be compared; keep `quota_left(work_root, now)`'s output identical); the flow write is skipped per section 4 (hash the JSON `fit_flow` would write after zeroing `observed_at` in a clone), and after a real write the memo records the hash, `observed_at` and the new stamp. `scan_pass` still returns the flow it built.
4. Keep `scan_pass`'s signature. Keep `scan_until` and every other function under 50 lines (split helpers into `pass_memo.rs`); `tracker.rs` is 280 lines today and must stay under 400.

## Tests

`pass_memo_tests.rs` / `tracker_tests.rs`:

- a second pass with no file change writes neither file (the files' stamps do not change) and returns the same totals;
- a checkpoint replaced on disk between passes (different length) is loaded from disk, not taken from the memo;
- an idle flow is written at `now`, skipped at `now + 5`, written again at `now + 60`;
- `flow.json` deleted between passes is written again on the next pass;
- an over-cap checkpoint save clears the memo, so the next pass loads from disk (the existing over-cap behaviour still holds);
- quota left is reused within 30 s when no quota file changed, and recomputed when the history file changed or 30 s passed;
- two work roots on one thread never share state.

Then run the existing suites that drive passes: `cargo test --lib tokens::`, `cargo test --test token_flow_contracts`, `cargo test --test token_flow_bounds_contracts`. Several of them call `scan_pass` repeatedly on one thread and one work root (corrupt checkpoint, resumes from offset, over-cap); they must pass unchanged. If one cannot, report it with its name and why rather than editing its assertion.

## Traps

- The memo must never outlive a change it cannot see: compare stamps before reuse, and clear on every failed or over-cap write.
- `atomic_write` renames a temp file into place, so the inode changes on every write: take the stamp after the write.
- Do not gate passes on dashboard subscribers; the tracker always runs.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass for your files. W1's edits may break the build for a while; report a failure inside W1's files rather than fixing it.
