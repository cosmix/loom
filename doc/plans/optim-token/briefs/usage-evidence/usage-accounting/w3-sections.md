# W3: polling tokens, orientation, requested model

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first.

## Files you own

- `src/commands/usage/sections/polling.rs`, `src/commands/usage/sections/polling_tests.rs` (new)
- `src/commands/usage/sections/lifecycle.rs`, `src/commands/usage/sections/lifecycle_orientation.rs` (new), `src/commands/usage/sections/lifecycle_tests.rs`
- `src/commands/usage/sections/agents.rs`, `src/commands/usage/sections/agents_tests.rs`
- `src/commands/usage/sections/mod.rs`
- `src/commands/usage/transcript_types.rs`, `src/commands/usage/transcript.rs`
- The `Transcript { .. }` literals in `src/commands/usage/forward_join_tests.rs`, `sections/rewrites_tests.rs`, `sections/totals_tests.rs` (two literals; both gain the field), `sections/by_model_tests.rs`, `sections/windows_tests.rs`, `sections/peaks_tests.rs` (one added field each; no other change)

Read-only: `src/commands/usage/sections/fmt.rs` (`percentile` at `:29`, generic over `Copy + Ord + Default`), `src/commands/usage/transcript_types.rs` (`Request`, `ToolUse`, `TokenUsage::resident`), `src/commands/usage/transcript_content.rs`.

## Step 1: the `requested_model` field

Add `pub requested_model: Option<String>` to `Transcript` (`transcript_types.rs:127-156`) after `agent_type`, with the doc text pinned in `common.md`. Initialize it to `None` in `transcript.rs` (the `Transcript {` literal at `:121`) and in every test literal listed above, plus the literal in `fn transcript` in `agents_tests.rs` and the one in `fn subagent` in `lifecycle_tests.rs`. W1 sets it in `usage/mod.rs::parse_all`; W4 writes the join.

## Step 2: tokens on observation-only requests (`polling.rs`)

Today `Polling` (`polling.rs:11-16`) counts repeated Bash commands and sleeps and attaches no tokens. Add:

```rust
#[derive(Debug, serde::Serialize, Default)]
pub struct ObservationTokens {
    pub requests: usize,
    pub resident_input_tokens: u64,
    pub output_tokens: u64,
    pub by_kind: Vec<ObservationKind>,
}
#[derive(Debug, serde::Serialize)]
pub struct ObservationKind {
    pub kind: String,
    pub requests: usize,
    pub resident_input_tokens: u64,
}
```

and `pub observation: ObservationTokens` on `Polling`. A request is observation-only when it has at least one tool use, its model is not `SYNTHETIC_MODEL` (`transcript_types.rs:99`), and every tool use classifies as one of these kinds; the request counts under the kind of its first tool use. Classify in this order:

1. `monitor`: tool name `Monitor`; `wakeup`: tool name `ScheduleWakeup`.
2. For `Bash`, on the command with leading whitespace trimmed: `subagents-wait` when it starts with `loom subagents watch`, `loom subagents wait` or `loom subagents list`; `status` when it starts with `loom status`, `loom request status` or `git status`; `sleep` when `sleep_seconds` (`:114-122`) parses it.
3. `repeated`: a Bash command that occurs at least 3 times in the same transcript (the threshold `repeated_commands` already uses, `:66`).

Resident tokens are `request.usage.resident()`; output is `request.usage.output`. `by_kind` lists kinds with at least one request, sorted by resident tokens descending then kind. `render` adds rows `observation-only requests`, `observation resident tokens` and one line per kind. Keep `polling.rs` under 300 lines; put tests in `polling_tests.rs` (declare `#[cfg(test)] #[path = "polling_tests.rs"] mod tests;`): a `loom subagents watch` request and a `Monitor` request are counted with their resident tokens; a request that mixes `Monitor` with `Edit` is not; a command repeated 3 times counts as `repeated`, 2 times does not.

## Step 3: orientation for main sessions and subagents (`lifecycle_orientation.rs`)

Today `requests_before_edit` (`lifecycle.rs:168-175`) covers subagents only and counts requests. Keep it, and add:

```rust
#[derive(Debug, serde::Serialize, Default)]
pub struct Orientation {
    pub main: OrientationScope,
    pub subagent: OrientationScope,
    pub skipped_read_only: usize,
}
#[derive(Debug, serde::Serialize, Default)]
pub struct OrientationScope {
    pub samples: usize,
    pub without_end: usize,
    pub median_requests: usize,
    pub p90_requests: usize,
    pub median_tokens: u64,
    pub p90_tokens: u64,
}
pub fn build(transcripts: &[Transcript]) -> Orientation
```

Orientation ends at the first request carrying one of: an edit tool (`Edit`, `Write`, `MultiEdit`, `NotebookEdit`, as `is_edit` at `:190-192`), a Bash command containing `git commit` or `loom stage commit`, or a spawn (tool name `Agent` or `Task`). The spawn counts because a stage main session delegates everything beyond a small change, so its first act is usually a spawn. The orientation of a transcript is the number of requests before that one and the sum of `usage.resident() + usage.output` over them. A transcript with no ending request counts in `without_end` and not in the samples. Skip transcripts without requests. Skip, and count in `skipped_read_only`, subagents whose `agent_type` is `loom-code-reviewer`, `loom-verifier`, `loom-advisor` or `Explore`: their recorded review round or verdict is their last act, so they have no orientation phase. Median and p90 use `fmt::percentile`.

`Lifecycle` (`lifecycle.rs:12-19`) gains `pub orientation: lifecycle_orientation::Orientation`, built in `lifecycle::build` from all transcripts (main and subagent, before the subagent filter) by calling `lifecycle_orientation::build(transcripts)`. Render two lines, `orientation (main): median N requests / T tokens, p90 ...` and the same for subagents, plus the skipped count. Declare the module in `sections/mod.rs` (`pub mod lifecycle_orientation;`). Tests in `lifecycle_tests.rs`: a main session whose third request spawns an `Agent` has orientation 2 requests and the token sum of the first two; a `loom-code-reviewer` subagent is skipped; a subagent that never edits counts in `without_end`.

## Step 4: requested model in `agents.rs`

`AgentModelRow` (`agents.rs:32-46`) gains `pub requested_model: String` after `agent_type`: `transcript.requested_model` or `unknown`. `by_agent_model` (`:170-207`) groups by `(agent_type, requested_model, model)`; sorting is unchanged with `requested_model` as the second tiebreak. `render` prints `{agent_type} / requested {requested_model} / {model}: ...`. `agents.rs` is 335 lines; stay under 360. Test in `agents_tests.rs`: two transcripts of one type with requested `sonnet` and `ambiguous` land in two rows.

Run once at the end: `cargo test --lib commands::usage::sections::` and report the result, every changed file and its line count after `cargo fmt`.
