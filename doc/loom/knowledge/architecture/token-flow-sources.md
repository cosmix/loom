# Token Flow Sources

> Token flow: sources, tracker, surfaces

## Token Usage Sources

- **Claude transcript lines.** Assistant lines carry `message.id`, `message.model`, `message.usage` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`) and an RFC 3339 timestamp. One API response spans several lines sharing `message.id`. Subagent transcripts are `<dir>/<session uuid>/subagents/agent-<agentId>.jsonl` with `isSidechain` true and `agentId`.
- **Claude effort.** Claude Code 2.1.300 assistant lines also carry top-level `effort` and `perTurnEffort` (equal on 2342 of 2344 sampled lines; one line carried only `perTurnEffort`). A subagent's lines carry its own effort, which can differ from its parent's. The sidecar `agent-<id>.meta.json` names the model as an alias (`haiku`) and has no effort key. Loom's Session record stores neither model nor effort (`models/session/types.rs`), so the transcript is the only per-agent source.
- **Uncached input is tiny by design.** `input_tokens` counts only what follows the last cache breakpoint: about 2 per API call in Claude Code sessions (three transcripts measured), so the web mix's "uncached input" row stays near twice the call rate while fresh tokens are mostly cache writes and output.
- **Codex rollouts.** `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<thread_id>.jsonl`, written line by line. `event_msg` `token_count` carries cumulative `total_token_usage` and per-call `last_token_usage`. `total_tokens = input_tokens + output_tokens`, so `cached_input_tokens` and `cache_write_input_tokens` are parts of `input_tokens`. `info` null is a rate-limit ping. `turn_context.payload.model` names the model and `turn_context.payload.effort` the reasoning effort. The prolite plan reports only a weekly window (`primary.window_minutes` 10080, `secondary` null).
- **Forward receipts.** `.loom/work/subagents/<stage>/forward-receipts.jsonl` (`loom/src/models/forward_receipt.rs`) record `codex_thread_id`; loom never passes `--ephemeral` to codex. PostToolUse writes a receipt only after the forwarder's foreground Bash call returns, so a running Codex job has no receipt (and no `codex_thread_id`) yet. Companion job records under `~/.codex/plugin-data/state/*/jobs/*.json` carry `threadId` while running.

## Status Pipeline and Web Contract

- The daemon status broadcaster re-reads files every second and never reads in-memory orchestrator state. A new status field needs a producer that persists a file the collector reads (the quota cache pattern).
- `loom/src/commands/status/web/model_tests.rs` `fixture_matches_serde_output` pins exact equality between the Rust serializer and `web/src/api/fixtures/snapshot.json`. `stageSummarySchema` and `snapshotSchema` are `.strict()`; `statusDataSchema` is not.
- `web/dist` is committed and embedded by `loom/build/assets.rs`. Cargo never runs bun; vite build output is deterministic.

## Quota History

- The quota poller samples every 180 s, so quota slopes need a window of about 30 minutes.
- `.loom/work/quota/history/<provider>.jsonl` flips the five-hour `resets_at` between two values one second apart on successive polls. `loom/src/quota/history.rs` `continuity_from_windows` marks any `resets_at` difference in either window as `Reset`, and the history store skips a row identical to the last one. A consumer that needs one reset period segments on the window's own `resets_at` with a tolerance instead of trusting `HistoryContinuity`.

## Maintainability Baseline

- `loom/maintainability-baseline.txt` rows are exact: `tests/maintainability/baseline.rs` fails when a recorded function grows and when it shrinks. The file is a `ratchet_files` entry in v2 plans, so editing a recorded function must keep its length or raise a TI-ratchet integrity event.

## Data Flow

The daemon owns a `tokens-tracker` thread (`spawn_token_tracker`, `loom/src/tokens/tracker.rs:72`, started with the quota poller in `daemon/server/lifecycle.rs` and joined at shutdown). Every `TRACKER_INTERVAL_SECS` (30) it reads the new bytes of every transcript that belongs to a stage and writes `.loom/work/tokens/flow.json` (a `TokenFlow`, `loom/src/tokens/model.rs`) plus `checkpoint.json`. The status collector reads `flow.json` into `StatusData.tokens`; the 1 s broadcast carries it to the TUI and the web dashboard. The tracker never runs inside the broadcaster, which only reads files (the quota-cache pattern). Source line formats and the Codex receipt lag are under Token Usage Sources above.

## Units

- **Bucket** `BUCKET_SECS` = 30; index = `floor(unix_secs / 30)`. Bucketing uses the timestamp on the line, never the read time, so a backlog read in one pass lands in its own past buckets; a line older than the series window adds to totals only.
- **Fresh tokens** = `input + cache_write + output` (`TokenUsage::fresh`). Every rate, series and headline is fresh unless labelled otherwise; cache reads appear only in the mix and the cache-hit percentage `cache_read / (cache_read + input + cache_write)` (undefined, shown `—`, at 0).
- **Rate** = sum of the last `RATE_BUCKETS` (10) buckets divided by 5, integer division per field. **Series** = fresh per bucket, oldest first: 120 values for a stage or agent (`STAGE_SERIES_LEN`), 60 for the plan (`PLAN_SERIES_LEN`, the per-bucket sum of each stage's last 60).
- **Active** agent: any of its last `ACTIVE_BUCKETS` (4) buckets is non-zero. Every UI reads `AgentFlow.active` verbatim; none derives it from `series`.
- Agent id: `main-` or `codex-` plus the first 8 characters of the session uuid or thread id; a subagent's id is its key. Agents of one stage that share a short form all get their full key (`flow_build.rs` `ordered`), so a UI never assumes 13-character ids.

## Tracker Pass

`scan_pass` (`tracker.rs:56`) runs `discover` (`discover.rs`: stage session `transcript_path`s and their `<stem>/subagents/` files, then Codex rollouts for each stage's receipt `codex_thread_id`), reads each source from its checkpointed offset (`tail.rs`), accumulates (`accumulate.rs`, `claude_line.rs`, `codex_line.rs`), prunes, builds the flow (`flow_build.rs`), estimates quota time left (`quota_left.rs`, `quota_points.rs`), saves the checkpoint and writes `flow.json`.

- **Claude dedupe.** One API response spans several lines sharing `message.id`; the per-file table keeps the field-wise maximum and adds only the increase. The table keeps the newest 4096 ids per file.
- **Codex.** Deltas between successive cumulative `total_token_usage`; a decrease makes the new total the delta. A thread whose rollout is not found is cached as a miss and retried at most every `MISS_RETRY_SECS` (600).
- **Reading.** Complete lines only (the offset stays at the start of an unfinished line); a line over `MAX_LINE_BYTES` (32 MiB) is skipped without buffering and counted unreadable; files open `O_NOFOLLOW | O_NONBLOCK` and `stat` uses `symlink_metadata`; the shutdown flag is checked between lines. A file that shrinks or changes inode resets only that file's state; a file no longer discovered keeps its agent's totals.
- **Unreadable lines.** Invalid JSON that contains `"usage"` (Claude) or `"token_count"` (Codex), or a usage object lacking numeric `input_tokens`/`output_tokens`, counts in `unreadable_lines`. An absent or `null` cache counter is 0 in both parsers (`optional_count`, `model.rs`); a present non-numeric one is unreadable.
- **Checkpoint.** `Checkpoint::save` refuses to write over `MAX_CHECKPOINT_BYTES` (8 MiB, what `load` reads) and returns `Saved::OverCap`; the pass then clears the message-id tables of agents whose file did not advance and measures again, and still over means nothing is written and the pass warning says so. `load_reporting` names why it fell back to an empty checkpoint (missing is silent). A checkpoint write failure still writes `flow.json`, then fails the pass.
- **Flow file.** Compact JSON, at most `MAX_FLOW_BYTES` (512 KiB, a quarter of the daemon's 2 MiB response frame). Over it, the tracker empties the `series` of inactive agents (totals stay); still over, the write is skipped and logged once, the file ages out and the collector publishes no `tokens`. An inactive agent's series can be empty or short, so UIs left-pad.
- **Shutdown and errors.** `scan_pass` has a pinned signature without a stop flag, so a thread-local (`TrackerLocal`, `tracker.rs:34`) carries the flag in and the pass warning out; `guarded_pass` runs the pass under `catch_unwind` and `log_change` logs a given warning once. `ensure_tokens_dir` refuses a symlinked `tokens` directory.

## Collector Boundary

`collector.rs:376` keeps the flow only when `now - observed_at` is within `0..=FLOW_STALE_SECS` (600; a future-dated flow is never fresh) and runs `sanitize_token_flow` (`status/data/sanitize_token_flow.rs`): strings go through `inline_safe`, every number is clamped to 2^53 - 1 (the web schema rejects a larger integer and then drops the whole frame), and series keep only their newest 120 or 60 values. `read_flow` opens through `crate::fs::safe_read::read_bounded`, so a FIFO or symlink never blocks the 1 s broadcaster. Callers name `read_flow` and `spawn_token_tracker` through their defining modules, because the source graph adds no call edge through the `crate::tokens::` re-export and `reachable` checks walk those edges.

## Quota Time Left

`quota_left::estimate(points, resets_at, now)` shows a duration, never a clock time ("~2h07m left", "lasts to reset", or nothing), and only for a provider that reports a five-hour window (Codex normally shows nothing).

1. `quota_points.rs` builds the points from `quota::read_history(work_root, provider, now - 1800, ...)`. It ignores `HistoryContinuity` and segments on the five-hour window alone: a point starts a new period when its `resets_at` differs from the previous point's by more than 300 s or its `used_percent` is lower (only the percent rule when either `resets_at` is null). The history flips `resets_at` by 1 s between polls, which `continuity_from_windows` marks `Reset`, so trusting it leaves one point.
2. The history store skips a row identical to the last, so the current snapshot is appended as the newest point when it was observed after the newest row.
3. Fewer than 2 points or a span under 360 s gives nothing; slope = `(last_used - first_used) / ((last_t - first_t) / 60.0)` in that f64 order; slope 0 or less gives `lasts_to_reset`; else `minutes_left = max(0, (100 - last_used) / slope - (now - last_t) / 60)` rounded down, and `lasts_to_reset` is set when `now + minutes_left * 60 >= resets_at`.

The 30-minute window exists because the poller samples every 180 s.

## Surfaces

- **Web.** `status.tokens` is optional in the strict schema, and a frame whose only defects sit under `tokens` drops that block alone (`web/src/api/schema.ts:256-271`). `TokenLine` (`header-lines.tsx`) is the header TOKENS rail: fresh/min, a 15-minute sparkline, plan total, cache hit. `TokenFlowPanel` (`token-flow-panel.tsx`) is the Token flow panel on `/` and `/ledger`: per-stage fresh bands, a mix row labelled "uncached input" and "cache hit (5 min)". The ledger rows carry a TOKENS cell, graph cards a `NodeTokens` trace (`graph/node-tokens.tsx`), the stage dialog a Tokens section (`stage-tokens.tsx`, mounted in `stage-sections.tsx`), and `QuotaMeters` takes `quotaLeft` for the label beside the Claude 5h meter. Helpers are in `web/src/lib/tokens.ts` (`fresh`, `lastValues`, `maxOf`, `compactTokens`, `quotaLeftText`).
- **TUI.** Header token summary (`header.rs`, the first thing to give way), TOKENS column, time-left suffix in the quota line (`quota.rs`, FULL and MEDIUM tiers only) and the `t` pane (`tokens_pane.rs`); layout in [status-data-model.md](status-data-model.md#what-the-live-tui-renders-today). Pane agent states are live or idle; the title row notes "no active agent" or "no token data yet".
- An executing stage shows its rate; every other stage with a flow shows its total dimmed in both UIs. A stage absent from `stages` shows `—` or blank, never 0.
- **Lower bound.** Numbers come from local transcripts: compaction and auxiliary calls may be missing, and sessions under another `CLAUDE_CONFIG_DIR` or `CODEX_HOME` may be invisible. Known gaps: [concerns/token-accounting-and-proof-defects.md](../concerns/token-accounting-and-proof-defects.md#token-flow-gaps).
