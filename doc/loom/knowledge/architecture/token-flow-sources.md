# Token Flow Sources

> Token usage sources: transcripts, rollouts, receipts

## Token Usage Sources

- **Claude transcript lines.** Assistant lines carry `message.id`, `message.model`, `message.usage` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`) and an RFC 3339 timestamp. One API response spans several lines sharing `message.id`. Subagent transcripts are `<dir>/<session uuid>/subagents/agent-<agentId>.jsonl` with `isSidechain` true and `agentId`.
- **Codex rollouts.** `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<thread_id>.jsonl`, written line by line. `event_msg` `token_count` carries cumulative `total_token_usage` and per-call `last_token_usage`. `total_tokens = input_tokens + output_tokens`, so `cached_input_tokens` and `cache_write_input_tokens` are parts of `input_tokens`. `info` null is a rate-limit ping. `turn_context.payload.model` names the model. The prolite plan reports only a weekly window (`primary.window_minutes` 10080, `secondary` null).
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
