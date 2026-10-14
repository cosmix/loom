# Token flow: shared specification

Every worker in every stage of `doc/plans/PLAN-token-flow.md` reads this file first. Where a brief and this file differ, this file wins. Paths are repo-relative.

Approved UI mockup (reference only, sample data): <https://claude.ai/artifact/BFDRMiWwtsv653hqsMPvED>.

## 1. What is being built

The daemon gets a `tokens-tracker` thread. Every 30 s it reads the new bytes of every transcript that belongs to a stage, and builds per-agent token buckets from them:

- Claude stage-session transcripts and their subagent transcripts.
- Codex rollout files of forwarded jobs.

It writes the result to `.loom/work/tokens/flow.json`. The status collector reads that file into a new `StatusData.tokens` field, which reaches the TUI (`loom status --live`) and the web dashboard (`loom status --web`) through the existing 1 s status broadcast. The tracker never runs inside the broadcaster: the broadcaster only reads `flow.json`.

## 2. Units and semantics

- **Bucket:** 30 s (`BUCKET_SECS`). Bucket index = `floor(unix_secs / 30)`.
- **Fresh tokens** = `input + cache_write + output`. Every rate, series and headline number is fresh unless it is labelled otherwise. Cache reads appear only in the mix and the cache-hit percentage.
- **Rate** is a `TokenUsage` per minute: the sum of the last 10 buckets (5 minutes) divided by 5, using integer division per field. The last bucket is the one that contains `observed_at`.
- **Series:** fresh tokens per bucket, oldest first, ending at the bucket containing `observed_at`.
  - Stages and agents carry 120 values (one hour).
  - The plan carries 60 values (30 minutes): the per-bucket sum over stages of each stage's last 60.
- **Cache hit %** = `cache_read / (cache_read + input + cache_write)` over a usage. It is undefined (shown as "—") when the denominator is 0.
- **Active:** an agent is active when any of its last 4 buckets (2 minutes) is non-zero.
- **Bucketing uses the timestamp on the line**, never the time it was read. A backlog read in one pass lands in its own past buckets. A line older than the series window adds to totals only.

## 3. Wire model (Rust, authoritative)

`loom/src/tokens/model.rs`, written verbatim from this listing by unit C3 of stage token-core (doc comments may be expanded):

```rust
//! Wire and file model of live token flow: `StatusData.tokens` and
//! `.loom/work/tokens/flow.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const BUCKET_SECS: i64 = 30;
pub const STAGE_SERIES_LEN: usize = 120;
pub const PLAN_SERIES_LEN: usize = 60;
pub const RATE_BUCKETS: usize = 10;
pub const ACTIVE_BUCKETS: usize = 4;
pub const TOKENS_DIR: &str = "tokens";
pub const FLOW_FILE: &str = "flow.json";
pub const MAX_FLOW_BYTES: u64 = 512 * 1024;
pub const FLOW_STALE_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl TokenUsage {
    /// input + cache_write + output, saturating.
    pub fn fresh(&self) -> u64 { /* saturating sum */ }
    /// Field-wise saturating add.
    pub fn add(&mut self, other: &TokenUsage) { /* ... */ }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind { Main, Subagent, Codex }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentFlow {
    pub id: String,
    pub kind: AgentKind,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub active: bool,
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageFlow {
    pub stage_id: String,
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
    pub agents: Vec<AgentFlow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanFlow {
    pub total: TokenUsage,
    pub rate: TokenUsage,
    pub series: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuotaProvider { Claude, Codex }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaLeft {
    pub provider: QuotaProvider,
    pub minutes_left: Option<u32>,
    pub lasts_to_reset: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenFlow {
    pub observed_at: i64,
    pub bucket_secs: i64,
    pub plan: PlanFlow,
    pub stages: Vec<StageFlow>,
    pub quota_left: Vec<QuotaLeft>,
    pub unreadable_lines: u64,
}

/// `<work_root>/tokens/flow.json`.
pub fn flow_path(work_root: &Path) -> PathBuf { /* ... */ }

/// The last written flow, or `None` when the file is missing, is a symlink or
/// sits under a symlinked `tokens` directory, is not a regular file, is larger
/// than MAX_FLOW_BYTES, cannot be read, or does not parse. It is read through
/// `crate::fs::safe_read::read_bounded(<work_root>/tokens, "flow.json",
/// MAX_FLOW_BYTES)`, which opens no-follow and non-blocking and checks the type
/// and size of the opened file (by `fstat`, and again while reading), so a FIFO
/// never blocks the caller. Never panics, never logs.
pub fn read_flow(work_root: &Path) -> Option<TokenFlow> { /* ... */ }
```

Serialization rules:

- No `skip_serializing_if` inside the model. `Option` fields serialize as `null`, so every key is always present (the web schema is `.strict()`).
- `StatusData` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub tokens: Option<crate::tokens::TokenFlow>` (`loom/src/commands/status/data/mod.rs`, after `quota`). The key is absent when there is no fresh flow.
- The collector sets `tokens` from `crate::tokens::model::read_flow(work_dir.root())`, kept only when `now.saturating_sub(observed_at)` is in `0..=FLOW_STALE_SECS` (`now` being the current unix time): a future-dated `observed_at` never goes stale, so it is never fresh, and the subtraction cannot overflow. It then runs `sanitize_token_flow` (`status/data/sanitize.rs`) on the flow, at that boundary, as that module does for the rest of the payload:
  - every string (`stage_id`, agent `id`, `agent_type`, `model`) is flattened with `inline_safe`;
  - every published number (each `TokenUsage` field of plan, stage and agent `total` and `rate`, every series value, `observed_at`, `bucket_secs`, `unreadable_lines`) is clamped to JavaScript's safe-integer range, 2^53 - 1 (`quota_left.minutes_left` is a `u32`, inside it already). The web schema rejects a larger integer and then drops the whole status frame;
  - each stage and agent series keeps only its newest `STAGE_SERIES_LEN` values and the plan series its newest `PLAN_SERIES_LEN`, so a planted `flow.json` cannot swell every frame. Shorter series are not padded.
- Callers name `read_flow` and `spawn_token_tracker` through their defining modules (`crate::tokens::model::`, `crate::tokens::tracker::`), never through the `crate::tokens::` re-export: the source graph adds no call edge for a qualified call through a re-export, and the `reachable` checks walk those edges.
- `MAX_FLOW_BYTES` is 512 KiB, a quarter of the daemon's 2 MiB response frame (`loom/src/daemon/wire.rs` `MAX_RESPONSE_BYTES`): a status frame over that limit is skipped for every client (`broadcast.rs` `frame_overflow`), freezing the TUI and web. Before writing, the tracker checks the serialized size; over the cap it empties the `series` of inactive agents (totals stay), and if still over it skips the write and logs once. A skipped write ages out after `FLOW_STALE_SECS`, and the collector then publishes no `tokens`: ordinary status always wins over token detail.
- `flow.json` is compact JSON (`serde_json::to_vec`, never `to_vec_pretty`), and the cap is measured on exactly the bytes written. Contract `oversized-flow-sheds-idle-series` (1600 idle subagents) depends on it.
- An inactive agent's `series` can therefore be empty or shorter than `STAGE_SERIES_LEN`. Every UI treats missing values as zeros and left-pads to the length it draws; none errors or skips the agent.
- An agent `id` is `main-` or `codex-` plus the first 8 characters of the session uuid or thread id (13 characters), and a subagent's is its key. Every agent of a stage that shares its short form with another agent of that stage gets its full key instead (`flow_build.rs` `ordered`), so a UI must not assume 13-character ids.

JSON shape, abbreviated:

```json
"tokens": {
  "observed_at": 1788523200, "bucket_secs": 30,
  "plan": { "total": {"input":0,"cache_write":0,"cache_read":0,"output":0}, "rate": {...}, "series": [60 numbers] },
  "stages": [ { "stage_id": "server", "total": {...}, "rate": {...}, "series": [120 numbers],
                "agents": [ { "id": "main-01ed9f4c", "kind": "main", "agent_type": null, "model": "claude-opus-5-5",
                              "active": true, "total": {...}, "rate": {...}, "series": [120 numbers] } ] } ],
  "quota_left": [ { "provider": "claude", "minutes_left": 127, "lasts_to_reset": false } ],
  "unreadable_lines": 0
}
```

## 4. Fixture values (pinned for both Rust and web)

Stage token-core writes this `tokens` block into `loom/src/commands/status/web/model_tests.rs` `fixture_status` and into `web/src/api/fixtures/snapshot.json`; `fixture_matches_serde_output` keeps the two equal. Stage web-tokens' contracts read these exact numbers. `server` is the fixture's executing stage.

- `observed_at` 1788523200 (the fixture time), `bucket_secs` 30, `unreadable_lines` 0.
- `plan`:
  - total `{input 120000, cache_write 1800000, cache_read 26000000, output 310000}`;
  - rate `{input 2000, cache_write 36000, cache_read 600000, output 5200}`;
  - series: 60 × 21600.
- `stages`: one entry, `stage_id` "server". Its total and rate equal the plan's; series 120 × 21600. Agents, in this order:
  1. id "main-01ed9f4c", kind main, agent_type null, model "claude-opus-5-5", active true:
     - total `{40000, 600000, 9000000, 100000}`;
     - rate `{600, 12000, 200000, 1600}`;
     - series 120 × 7100.
  2. id "a04baa183f888b773", kind subagent, agent_type "loom-software-engineer", model "claude-sonnet-5-5", active true:
     - total `{50000, 900000, 13000000, 160000}`;
     - rate `{1200, 21000, 350000, 3000}`;
     - series 120 × 12600.
  3. id "codex-01a1396b", kind codex, agent_type null, model "gpt-6.1-sol", active true (its last 4 buckets are non-zero, so section 2's Active rule makes it true):
     - total `{30000, 300000, 4000000, 50000}`;
     - rate `{200, 3000, 50000, 600}`;
     - series 120 × 1900.

  Usages above are listed as `{input, cache_write, cache_read, output}`.
- `quota_left`: `[{provider "claude", minutes_left 127, lasts_to_reset false}]`.

Derived numbers the UIs display from this fixture:

- Plan fresh rate 43200/min, shown as "43.2k".
- Plan fresh total 2230000, shown as "2.23M".
- Cache hit 26000000 / (26000000 + 120000 + 1800000) = 93.1%.
- Agents live: 3 of 3.

Every UI reads `AgentFlow.active` verbatim for live/idle, the "agents live" count and the rate dash; none derives activity from `series`.

## 5. Source formats (captured 2026-10-14 from real files)

**Claude transcript line** (stage-session transcript `<dir>/<uuid>.jsonl`; subagent transcripts are `<dir>/<uuid>/subagents/agent-<agentId>.jsonl`):

```json
{"type":"assistant","timestamp":"2026-09-30T21:09:35.379Z","isSidechain":true,"agentId":"a04baa183f888b773","sessionId":"01ed9f4c-608e-4e2c-8edf-05d26adcd4d6","message":{"id":"msg_011CfaJtKtxUSKsEhtf2ECes","model":"claude-sonnet-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":14477,"cache_read_input_tokens":4542,"cache_creation":{"ephemeral_5m_input_tokens":14477,"ephemeral_1h_input_tokens":0},"output_tokens":20,"service_tier":"standard","inference_geo":"not_available"}}}
```

Mapping:

- `input` = `input_tokens`
- `cache_write` = `cache_creation_input_tokens`
- `cache_read` = `cache_read_input_tokens`
- `output` = `output_tokens`

`input_tokens` and `output_tokens` are required. A missing or `null` cache field counts as 0; a cache field that is present and not a non-negative integer (for example the string `"20"`) makes the line unreadable.

The agent's model is the `message.model` of its latest usage line, except the literal `<synthetic>` (`SYNTHETIC_MODEL`, `loom/src/commands/usage/transcript_types.rs`): Claude Code writes it on an API-error row that has all-zero usage, and such a row never sets the model.

One API response is written as several lines that share `message.id`, each repeating `usage`. The tracker keeps the field-wise maximum per `message.id` and adds only the increase, so a response counts once and a later line with a larger `output_tokens` adds the difference. A usage line without `message.id` counts on its own.

**Codex rollout** (`~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<thread_id>.jsonl`, for example `rollout-2026-10-14T10-58-07-01a1396b-2aa4-7d93-9e18-d54685ff45df.jsonl`):

```json
{"timestamp":"2026-10-14T08:00:28.716Z","type":"session_meta","payload":{"id":"01a1396b-2aa4-7d93-9e18-d54685ff45df","cwd":"<cwd>","originator":"codex-tui"}}
{"timestamp":"2026-10-14T08:00:36.048Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":21255,"cached_input_tokens":12160,"cache_write_input_tokens":0,"output_tokens":110,"reasoning_output_tokens":0,"total_tokens":21365},"last_token_usage":{"input_tokens":21255,"cached_input_tokens":12160,"cache_write_input_tokens":0,"output_tokens":110,"reasoning_output_tokens":0,"total_tokens":21365},"model_context_window":475000}}}
{"timestamp":"2026-10-14T08:00:46.390Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":43274,"cached_input_tokens":33280,"cache_write_input_tokens":0,"output_tokens":206,"reasoning_output_tokens":0,"total_tokens":43480},"last_token_usage":{"input_tokens":22019,"cached_input_tokens":21120,"cache_write_input_tokens":0,"output_tokens":96,"reasoning_output_tokens":0,"total_tokens":22115},"model_context_window":475000}}}
{"type":"turn_context","payload":{"model":"gpt-6.1-sol"}}
```

- `total_tokens = input_tokens + output_tokens`, so `cached_input_tokens` and `cache_write_input_tokens` are parts of `input_tokens`, and `reasoning_output_tokens` is part of `output_tokens`.
- Mapping:
  - `input` = `input_tokens - cached_input_tokens - cache_write_input_tokens` (saturating)
  - `cache_write` = `cache_write_input_tokens` (0 when absent or `null`)
  - `cache_read` = `cached_input_tokens` (0 when absent or `null`)
  - `output` = `output_tokens`
- The tracker uses deltas between successive `total_token_usage` values, so a missed line loses nothing. When any field decreases, the new total itself is the delta (a new baseline).
- A `cached_input_tokens` or `cache_write_input_tokens` that is present and not a non-negative integer makes the line unreadable, as it does for Claude.
- `"info": null` is a rate-limit-only ping and is not usage.

**Forward receipts:** a stage's receipts live at `.loom/work/subagents/<stage_id>/forward-receipts.jsonl`. `loom/src/models/forward_receipt.rs` gives the path (`receipts_path`) and the decoders. A `ForwardReceipt` carries `codex_thread_id: Option<String>`. The rollout for a thread is the file under the Codex sessions root whose name ends in `-<thread_id>.jsonl`. Codex never runs with `--ephemeral` in loom (checked: no occurrence in `loom-hooks/` or `loom/src`).

PostToolUse writes a receipt only after the forwarder's foreground Bash call returns (`loom-hooks/post-tool-use.sh`; `forward_receipt.rs` skips calls without a result). A running Codex job is therefore invisible until its forward returns (up to the 540 s wrapper deadline); its lines then land in their own past buckets by timestamp. This lag is accepted (plan Decision 12).

## 6. Quota time left

The UI shows a duration, never a clock time:

- "~2h07m left", or "~43m left" when under an hour;
- or a dim "lasts to reset";
- or nothing.

It is shown only for providers that report a five-hour window. The Codex prolite plan reports only a weekly window, so Codex normally shows nothing.

`loom/src/tokens/quota_left.rs::estimate(points, resets_at, now)` computes it:

1. `points` are `(observed_at, used_percent)` pairs of one provider's five-hour window, oldest first, within one reset period. The tracker builds them (W1) from `crate::quota::read_history(work_root, provider, now - 1800, Some(now))`:
   - It ignores `HistoryContinuity`. Real history alternates the five-hour `resets_at` between two values one second apart on successive polls, and `loom/src/quota/history.rs` marks any `resets_at` difference in either window as `Reset`, so a run "since the last Reset" holds one point.
   - It segments on the five-hour window alone: walking oldest to newest, a point starts a new period when its five-hour `resets_at` differs from the previous point's by more than 300 s, or its `used_percent` is lower than the previous point's. When either point's five-hour `resets_at` is null, only the `used_percent` rule applies. Only the newest period is passed to `estimate`, with the newest point's five-hour `resets_at` (`None` when null, so `lasts_to_reset` stays false).
   - The seven-day window plays no part. `continuity_from_windows` returns `Unknown` (which outranks `Reset`) when the seven-day window appears or disappears, so a real five-hour reset can arrive without any `Reset` mark; the five-hour rules above still split it (contract `quota-left-ignores-pre-reset-points`).
   - The history store skips a row identical to the last one (`history_store.rs`), so flat usage leaves the newest row older than the latest poll. When `crate::quota::read_snapshot(work_root)` carries that provider with a five-hour window observed after the newest history row, its `(observed_at, used_percent)` is appended as the newest point (it is subject to the same segmentation).
   - A provider gets an entry only when its newest point has a five-hour window.
2. Keep only points with `observed_at >= now - 1800`.
3. Fewer than 2 points, or a span shorter than 360 s, gives `{None, false}`, and the UI shows nothing.
4. The slope is computed from the endpoints, in percentage points per minute, in exactly this f64 order: `(last_used - first_used) / ((last_t - first_t) as f64 / 60.0)`. Another order can differ in the last bit and floor one minute lower.
5. A slope of 0 or less gives `{None, true}`.
6. Otherwise `minutes_left = max(0, (100 - last_used) / slope - (now - last_t) / 60)`, rounded down.
7. `lasts_to_reset` is true when `resets_at` is known and `now + minutes_left * 60 >= resets_at`.

The window is 30 minutes, not 5. The quota poller samples every 180 s (`loom/src/quota/poller.rs` `POLL_INTERVAL`), so a 5-minute window would hold at most two samples. The tooltip and legend text say "at the last 30 min average".

Display tone:

- "lasts to reset": dimmed.
- Under 30 minutes: blocked (red).
- Otherwise: warning (amber).

## 7. Corner cases the tracker must handle

| Case | Rule |
| --- | --- |
| Partial last line | Consume complete lines only, up to the last `\n`. The offset stays at the start of the unfinished line. |
| One response, several lines (Claude) | Per-file `message.id` table with the field-wise max. Keep the most recent 4096 ids per file; older ids are evicted. |
| Codex cumulative counters | Deltas between totals; a decrease is a new baseline. |
| File shrinks or inode changes | Drop that file's accumulated state (its agent's totals and buckets), reset the offset to 0 and re-read the whole file. |
| Subagent file appears late | List `<transcript stem>/subagents/` on every pass. New files start at offset 0. |
| Daemon paused or laptop asleep | Bucket by line timestamp. |
| Daemon restart | Checkpoint `.loom/work/tokens/checkpoint.json` (offsets, inodes, per-file dedupe tables, Codex last totals, per-agent totals and buckets, unreadable count) after every pass, atomically. A missing, unparseable, wrong-version, symlinked or oversized (over 8 MiB, checked before reading) checkpoint means a full rescan; all but a missing one are named in the pass warning. |
| Checkpoint growth | Buckets older than the bucket containing `observed_at` minus `STAGE_SERIES_LEN` are dropped before writing (older usage lives in totals only). The per-file dedupe table keeps the 4096 most recent ids. |
| Checkpoint over the cap | `Checkpoint::save` refuses to write more than the 8 MiB that `load` reads, measured on the exact compact JSON it would write. Over it, the pass empties the message-id table of every agent whose file did not advance this pass and measures again. Still over: nothing is written, the next pass starts from the checkpoint before this one, and the pass's warning says so (logged once, like any warning). Clearing a finished file's table means a later repeated id in that file could count again. |
| Over-long line | A line over 32 MiB (`tail::MAX_LINE_BYTES`) is skipped without buffering more than the cap, the offset moves past it, and it counts in `unreadable_lines`. A line still unfinished is left for a later read, as for any partial line. |
| File no longer discovered | Discovery only adds agents. A checkpointed agent whose file is no longer found (transcript deleted, `transcript_path` replaced by a later heartbeat, rollout removed) keeps its totals and buckets; its file is skipped, never reset. Only a shrink or inode change on a file that still exists resets that file's state. |
| Unresolved Codex thread | A thread whose rollout is not found is cached as a miss with the pass time; the sessions root is walked again for it at most every 10 minutes, never every pass. |
| Format drift | A Claude line that is invalid JSON and contains `"usage"`, or an assistant line whose `usage` lacks numeric `input_tokens` or `output_tokens` or has a present non-numeric cache field, counts in `unreadable_lines`. An absent or `null` cache field is 0, for Claude and for Codex. So does a Codex line that contains `"token_count"` and is invalid JSON, or whose `info` object lacks a numeric `total_token_usage`. Valid JSON lines that merely mention the word are not usage and not unreadable. |
| No transcript path yet | The stage is absent from `stages`. The UI shows "—", never 0. |
| Untrusted strings | `agent_type` and `model` pass through `crate::context::untrusted::inline_safe`. Agent ids come from file names and must satisfy `crate::models::forward_receipt::is_safe_id`; files with unsafe names are skipped. A transcript path in a logged warning passes through `inline_safe` too. |
| Watching cost | Stat size and inode first (without following a symlink); open only files that grew or changed, with `O_NOFOLLOW | O_NONBLOCK`, and stop between lines when the tracker's shutdown flag is set. |
