# W1: the `agent_state` module (step A3)

Read `common.md` in this directory first. You are a `loom-software-engineer`. You run in wave 1 beside W2 (`src/tokens/**`), which shares the crate: if a build fails inside `src/tokens/`, it is W2's half-written code; check again later rather than touching it.

## Goal

One library module, `loom/src/agent_state/`, holds the subagent-state rules that two callers need: `loom subagents` (which reads a whole transcript file) and the session bridge (which reads one line at a time, W3). Behaviour of `loom subagents` and of the forward-receipt reader does not change.

## Files owned

`src/agent_state/**` (new), `src/lib.rs`, `src/commands/subagents/classify.rs`, `src/commands/subagents/classify_progress_tests.rs` (new), `src/commands/subagents/classify/entry.rs`, `src/commands/subagents/wait/stall.rs`, `src/models/forward_receipt/transcript.rs`, `src/models/forward_receipt/payload.rs`.

Read only: `src/commands/subagents/classify_tests.rs`, `classify_*_tests.rs`, `wait/stall_tests.rs`, `render_tests.rs`, `forward_jobs_tests.rs`, `models/forward_receipt/transcript_tests.rs`, `payload_tests.rs`. They must pass unchanged.

## What exists (verified 2026-11-10)

- `classify.rs:97` `pub const DEFAULT_DONE_DEBOUNCE_SECS: u64 = 180`, `classify.rs:101-146` `pub enum SubagentState` with `label()`, `classify.rs:231-279` `analyze_with_evidence_at_ceiling` (reads every entry through `entry::read_entries`, takes `entries.last()`, its timestamp or the file mtime for idle time, `resolve_state`, then `transcript_activity` counts assistant entries and takes `last_tool_used`), `classify.rs:332-339` `resolve_state` (private).
- `classify/entry.rs:119-143` `classify_last` and `classify_assistant`; `:146-156` `last_tool_used`; `:158-181` the private helpers `entry_type`, `message_content_blocks`, `block_type`, `block_text`, `block_tool_name`; `:89-95` `timestamp`. `READ_COUNT` (`:14-22`) stays with `read_entries`: `forward_jobs_tests.rs:52,65` read it through `classify::read_count`.
- `wait/stall.rs:121-147` `stall_reason` (private, already takes `budget_secs`), `:149-167` `tool_wait_reason`, `:45` `BASH_STALL_FLOOR_SECS`.
- `models/forward_receipt/transcript.rs:198-206` and `payload.rs:84-88` both read `backgroundTaskId` or `background_task_id` as a string filtered by `is_safe_id`.
- `lib.rs` lists modules alphabetically from line 4 (`pub mod assets;`).

## Steps

1. **`src/lib.rs`.** Add `pub mod agent_state;` before `pub mod assets;`.
2. **`src/agent_state/mod.rs`.** A module doc of two or three lines (what the module is for and its two callers), the three `pub mod` lines and the `pub use` lines of `common.md`.
3. **`src/agent_state/progress.rs`.** Move, unchanged in behaviour and doc: `SubagentState` with its derives and `label()`, `DEFAULT_DONE_DEBOUNCE_SECS`, `classify_last`, `classify_assistant`, `resolve_state` (now `pub fn resolve_state(last: &Value, idle_secs: i64, debounce_secs: u64) -> SubagentState`) and the helpers `entry_type`, `message_content_blocks`, `block_type`, `block_text`, `block_tool_name` (as `pub(crate)` where `entry.rs` still needs them). Keep the classification table doc of `classify.rs:8-15` with `classify_last`. Then add `AgentProgress`:
   - `observe(entry)` takes every parsed JSONL value, of any type, in file order. It sets `last` to a clone of the entry and `last_at` to that entry's own `timestamp` (`None` when it has none; never the latest timestamp seen earlier), adds one to `assistant_entries` when the entry is an `assistant`, and, when an assistant entry holds a `tool_use` block, sets `last_tool` to the name of its last `tool_use` block. That is `last_tool_used`'s rule (the most recent tool call, even when a later user result exists) as a fold.
   - `idle_secs(now)` is `now - last_at`, at least 0, or `None` without `last_at`.
   - `state(now, debounce)`: `Unknown` when nothing was observed, else `resolve_state(last, idle_secs(now).unwrap_or(0), debounce)`.
4. **`src/commands/subagents/classify.rs`.** Replace the moved items with `pub use crate::agent_state::progress::{SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};` and a private `use crate::agent_state::progress::{resolve_state, AgentProgress};` (the test modules reach items through `use super::*`, which sees private imports). In `analyze_with_evidence_at_ceiling`, fold the entries into one `AgentProgress` right after `read_entries`; take `last` from `progress.last()`, idle from `progress.idle_secs(Utc::now())` with `idle_since_mtime(path)` as today's fallback, `turns` from `assistant_entries()` and `last_tool` from `progress.last_tool()`. `metrics::extract(&entries)` and the forward-job reading keep their whole-file inputs. Keep the function at most 50 lines; split a helper out if needed. `open_tool_call` and `is_done_entry` call the moved `classify_last`.
5. **`src/commands/subagents/classify/entry.rs`.** Keep `read_entries` with `READ_COUNT`, `read_tail_entries`, `parse_lines`, `timestamp`, `is_assistant`, `is_message`, `text_blocks` and `last_tool_used`, importing the moved helpers from `agent_state::progress` (no second copy). `pub(crate) use` paths that `commands/subagents/mod.rs:34` re-exports (`is_assistant`, `text_blocks`) keep working.
6. **`src/agent_state/stall.rs`.** Move `stall_reason`, `tool_wait_reason` and `BASH_STALL_FLOOR_SECS` with their docs; `stall_reason` becomes `pub`. `wait/stall.rs` imports `stall_reason` (and `BASH_STALL_FLOOR_SECS` if `stall_tests.rs` names it) privately, so `claude_stall` and the tests compile unchanged.
7. **`src/agent_state/background_id.rs`.** `pub fn background_task_id(tool_use_result: &Value) -> Option<String>`: the string at `backgroundTaskId`, else at `background_task_id`, kept only when `is_safe_id` accepts it. `transcript.rs` `add_result` calls it inside its `find_map` over the metadata values; `payload.rs` `result_from` builds `Value::Object(response.clone())` once and passes it to both `background_task_id` and `collect_strings`. The output-path check in `forward_receipt/locator.rs` stays where it is.
8. **Tests.**
   - In `src/agent_state/progress_tests.rs` (declared `#[cfg(test)] mod progress_tests;` in `progress.rs`, or through `#[path]` as the subagent tests are): unit cases for `observe` and `state` (an entry without a timestamp, a text-only assistant entry inside and past the debounce, a `tool_use` entry, a trailing `attachment` row that makes the last entry `Unknown`).
   - In a new `src/commands/subagents/classify_progress_tests.rs` (declared next to the other test modules at the end of `classify.rs`): for each of `tests/fixtures/plan_session/transcripts/p1-background-agent-agent-1.jsonl`, `p1-interrupt-agent-1.jsonl`, `p1-status-agent-1.jsonl` and `subagent-agent.jsonl`, the state, assistant-entry count and last tool from folding `observe` over the file's lines equal `analyze(path, ..)`'s `state`, `turns` and `last_tool` at the same debounce. Lines that are not JSON are skipped, as `parse_lines` skips them.
   - In `src/agent_state/background_id_tests.rs`: over the user records of `tests/fixtures/plan_session/transcripts/p1-status.jsonl` that carry a `toolUseResult` object, `background_task_id` yields exactly `badtfi3uv` and `bx0h460uz` (the two Bash launches; the Agent launch carries no task id), and an unsafe value such as `../x` yields `None`.
9. **Run** `cargo test --lib agent_state:: commands::subagents:: models::forward_receipt::` once your code is ready; every existing test must pass unchanged.

## Done when

- The three moved items have one definition each, in `agent_state`; `classify.rs` and `wait/stall.rs` name them through imports; `classify::DEFAULT_DONE_DEBOUNCE_SECS` and `classify::SubagentState` paths still resolve.
- `rg -n 'backgroundTaskId' src/models` finds only test files.
- The tests above pass, and so do the existing `commands::subagents::` and `models::forward_receipt::` tests.

## Traps

- `entries.last()` is the last parsed row of any type, a bookkeeping row included; the fold must keep that, or `Unknown` cases change.
- Do not move `idle_since_mtime`: the bridge has no file mtime to fall back on.
- `classify.rs` must stay under 400 lines and every function under 50 after `rustfmt`.
