# W1: token tracker engine (Claude, opus, effort xhigh)

Read `doc/plans/briefs/token-flow/common.md` in full first; sections 2, 5 and 7 are your spec. You implement the stateful part. Units C1-C4 have already landed in wave 1, so build on them:

- `tokens/quota_left.rs` `estimate`
- `tokens/codex_line.rs` `parse_line`, `CodexTotals`
- `tokens/claude_line.rs` `parse_line`
- `tokens/model.rs`

Do not edit those four files. If one is wrong, report it to the orchestrator with the failing input.

## Files you own

- `loom/src/tokens/mod.rs`: add your module declarations. `tracker` must be `pub mod tracker;` (callers name `crate::tokens::tracker::spawn_token_tracker`; common.md section 3), and re-export `scan_pass`, `spawn_token_tracker`, `TRACKER_INTERVAL_SECS` and `CHECKPOINT_FILE` with `pub use tracker::{...};` for the contracts' `loom::tokens::scan_pass`. Keep `pub use model::*;` and the existing declarations.
- `loom/src/tokens/tail.rs`: incremental file reader.
- `loom/src/tokens/accumulate.rs`: per-agent totals, buckets and the dedupe tables.
- `loom/src/tokens/discover.rs`: the sources of each stage.
- `loom/src/tokens/checkpoint.rs`: checkpoint model and atomic IO.
- `loom/src/tokens/tracker.rs`: `scan_pass` and the thread.
- One sibling `*_tests.rs` per module above.
- `loom/src/commands/usage/codex_discovery.rs` and `loom/src/commands/usage/mod.rs`: `mod codex_discovery;` in `usage/mod.rs` is private, so make it `pub(crate) mod codex_discovery;` and `codex_root` `pub(crate)`; widening the fn alone does not make it reachable from `crate::tokens`.

Split further if a file would pass 360 lines. The 400-line limit is enforced by `cargo test --test maintainability`, and `cargo fmt` can add lines.

## Public surface (pinned; the contracts call it)

```text
// loom/src/tokens/tracker.rs, re-exported from tokens/mod.rs
pub const TRACKER_INTERVAL_SECS: u64 = 30;
pub const CHECKPOINT_FILE: &str = "checkpoint.json";
pub fn scan_pass(work_root: &Path, codex_sessions_root: Option<&Path>, now: i64) -> anyhow::Result<TokenFlow>
pub fn spawn_token_tracker(work_root: PathBuf, shutdown: Arc<AtomicBool>) -> JoinHandle<()>
```

`work_root` is the `.loom/work` directory, the value `WorkDir::root()` returns.

## `scan_pass`

1. Load `<work_root>/tokens/checkpoint.json`. Missing, unparseable, wrong `version` or a symlink means starting from empty state.
2. Discover each stage's sources:
   - **Sessions.** Load every session record with `crate::commands::status::data::load_all_sessions(&WorkDir)`, building the `WorkDir` with `WorkDir::new(work_root)` exactly as `daemon/server/status.rs` `collect_status` does with the daemon's `.loom/work` path (no detour through the project root; a unit test proves it resolves back to `work_root` for a TempDir fixture). Keep the ones with a `stage_id` that passes `is_safe_id` and a `transcript_path`.
   - **Main transcript.** Each such session's transcript file is a main agent. Its id is `main-` plus the first 8 chars of the file stem.
   - **Subagents.** Each `agent-<id>.jsonl` under `<parent dir>/<stem>/subagents/` (`crate::commands::subagents::resolve::list_agent_files`, `agent_id_from_path`) is a subagent with that id. Look its `agent_type` up through `crate::commands::subagents::ledger::StartedAgentTypeIndex` (load once per pass) with `get_metadata(agent_id, parent_session_id)`, where `parent_session_id` is the main transcript's file stem (the Claude session uuid). `StartedAgentTypeIndex::get` is `#[cfg(test)]` and unusable here; ledger rows are scoped, and an unscoped lookup finds nothing.
   - **Codex jobs.** Read each stage's forward receipts: `crate::models::forward_receipt::receipts_path(work_root, stage_id)`, then `load_receipts` in `models/forward_receipt/load.rs`. Every distinct `codex_thread_id` is a codex agent with id `codex-` plus its first 8 chars.
   - **Rollout files.** Resolve each thread's rollout under the sessions root: `codex_sessions_root`, else `<codex root>/sessions`, with the codex root from `crate::commands::usage::codex_discovery::codex_root(None)` (widened as described under Files). The rollout is the file whose name ends in `-<thread_id>.jsonl`. Cache resolved paths in the checkpoint. Walk the root at most once per pass, and only for threads that are unresolved and whose cached miss is older than 10 minutes (common.md section 7): a thread whose rollout never appears must not cost a full walk every 30 s.
   - **Agents only accumulate.** A checkpointed agent whose file is not found this pass keeps its totals and buckets; skip the file, never reset it (common.md section 7).
3. Read each file:
   - Stat it. If the inode changed, or the size is below the saved offset, drop that file's state and start again from offset 0.
   - Read from the offset to the last `\n` only, and save the new offset.
   - Claude lines go through `claude_line::parse_line`, with the per-file `message.id` max table (4096 most recent ids).
   - Codex lines go through `codex_line::parse_line`, tracking the last total and model per file.
   - Count every `Unreadable` line.
   - Put usage into buckets by line timestamp, as common.md section 2 says.
   - The agent `model` is the last model seen in that file.
   - Pass `agent_type` and `model` through `crate::context::untrusted::inline_safe`.
4. Build the `TokenFlow` for `now`:
   - series lengths, rate, `active` and plan sums exactly as common.md section 2;
   - stages sorted by id;
   - within a stage, main agents first, then subagents, then codex, each group by first-seen timestamp.
5. Fill `quota_left` for `claude` and `codex` exactly as common.md section 6 step 1 says: read `crate::quota::read_history(work_root, provider, now - 1800, Some(now))`, ignore `HistoryContinuity`, segment on the five-hour window alone (a `resets_at` jump over 300 s or a `used_percent` drop starts a new period; with a null `resets_at` on either side only the drop rule applies; the seven-day window plays no part, so a reset that `history.rs` marks `Unknown` because the weekly window vanished still splits: contract `quota-left-ignores-pre-reset-points`), append the newer `crate::quota::read_snapshot(work_root)` reading, and call `quota_left::estimate` on the newest period with the newest point's five-hour `resets_at`. Never keep "the run since the last Reset": real history flips `resets_at` by 1 s between polls, so that run is one point and the label would never appear (contract `quota-left-survives-reset-jitter`).
6. Before writing, drop buckets older than `STAGE_SERIES_LEN` before the observed bucket and enforce `MAX_FLOW_BYTES` as common.md section 3 says: serialize compactly with `serde_json::to_vec`, measure those bytes, and over the cap empty the `series` of every agent whose `active` is false (keep its totals, keep active agents' series), re-measure, and if still over skip the `flow.json` write (the checkpoint is still written) and log once (contract `oversized-flow-sheds-idle-series`). Write the checkpoint, then `flow.json`, each atomically with `crate::quota::cache::atomic_write`. Create `<work_root>/tokens` with mode 0700 and refuse a symlinked directory the way `loom/src/quota/history_store.rs` `ensure_history_dir` does: `reject_symlink(&dir)` BEFORE `create_quota_dir(&dir)`, then `symlink_metadata` must show a real directory. `create_quota_dir` alone is NOT enough: `DirBuilder::recursive(true)` accepts an existing symlink to a directory, and `reject_symlink` checks only the last path component (contract `symlinked-tokens-dir-is-refused`). Read the checkpoint only through the same refusal plus an 8 MiB size cap, the same way `read_flow` reads `flow.json` (common.md section 3): `symlink_metadata` must show a regular file before opening (a FIFO would block the pass), and the read goes through `take(8 MiB + 1)`. Return the flow.

## The thread

`spawn_token_tracker` names the thread `tokens-tracker`. Its loop:

- Runs `scan_pass(&work_root, None, now)` immediately, then every `TRACKER_INTERVAL_SECS`.
- Sleeps in 250 ms slices so a shutdown is noticed promptly, as `quota/poller.rs` `run_quota_poller` does (`SLEEP_SLICE`).
- `scan_pass` takes no flag, so the thread wraps it: check the shutdown flag between passes, and give `scan_pass`'s internal per-file loop a private variant that stops early when the flag is set, so a daemon stop never waits on a long pass. The daemon joins each producer thread for up to 5 s, sequentially (`daemon/server/lifecycle.rs`), inside the 30 s stop deadline of `commands/stop.rs`.
- Each pass runs inside `std::panic::catch_unwind` (with `AssertUnwindSafe`): a panic is logged like an error and the loop continues. Without it, one panic ends the thread and token data silently vanishes after `FLOW_STALE_SECS`.
- On error, writes one `eprintln!("tokens: {e}")` line and keeps the loop alive. The daemon's stderr is its log, as the quota poller's `eprintln!("quota: ...")` lines show. Log a repeated identical error once until it changes. Never panic.

W2 adds the call in `daemon/server/lifecycle.rs`; do not edit that file.

## Tests you write (unit tests beside each module)

Cover each section 7 row with a focused test against TempDir fixtures. Build the fixture lines from the samples in common.md section 5, never from memory.

- **Partial line:** a half line is not counted until its newline arrives, then counted once.
- **Repeated `message.id`:** counted once; a later larger `output_tokens` adds only the difference.
- **Truncate and rewrite:** totals equal the new content.
- **Backlog by timestamp:** a 10-minute-old line lands 20 buckets back.
- **Codex deltas and decrease:** deltas are counted, and a decrease starts a new baseline.
- **Unreadable count.**
- **Late subagent file:** picked up on the next pass.
- **Second pass with no change:** adds nothing.
- **Corrupt checkpoint:** a full rescan with no double count.
- **Unsafe agent file name:** skipped.
- **Thread shutdown:** the thread exits within 1 s of the flag being set.
- **Quota history jitter:** three rows whose five-hour `resets_at` alternate by 1 s yield one estimate; a jump over 300 s or a percentage drop starts a new period; a newer `read_snapshot` reading is appended.
- **Missing file keeps totals:** delete a counted transcript; the next pass still reports its agent's totals.
- **Symlinked checkpoint and oversize checkpoint:** each means a full rescan, never a read through the link.
- **Flow size cap:** a flow over `MAX_FLOW_BYTES` drops inactive agents' series before writing; one still over the cap is not written.
- **Two stages and decay:** the plan rate and total sum both stages; a later pass with no new lines reports rate 0 and `active` false with totals kept (contract `plan-rate-sums-stages-and-expires`).
- **Checkpoint that is a directory or FIFO:** a full rescan, and the pass does not block.

Prove with `cargo test --lib tokens::` from `loom/`.

## Traps

- A handed-off stage has several session records. Count each one; do not keep only the stage's current session. `transcript_path` is NOT set once: `Session::record_heartbeat` (`models/session/methods.rs`) replaces it whenever a heartbeat carries a new path, so an agent whose file is no longer named keeps its checkpointed totals (common.md section 7).
- `WorkDir::new` on a bare TempDir resolves to `<tmp>/.loom/work`, which does not exist yet. In tests, create `<tmp>/.loom/work` first and pass that path as `work_root`.
- No test may read the real `~/.claude` or `~/.codex`. Always pass `codex_sessions_root`, and point session records at TempDir files.
- No dead code: every helper you add is called.
