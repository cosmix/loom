# W4 (codex unit): spawn ledger join for the requested model

Tier: codex terra (`loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`). One file plus its test. Paths are relative to `loom/`.

## Owns

- `src/commands/usage/spawn_join.rs` (new)
- `src/commands/usage/spawn_join_tests.rs` (new)

Read for patterns: `src/provider_data/ledger.rs` (`StartedAgentTypeIndex::load` at lines 90-103, `stage_directories` at lines 274-293, `json_lines`). Another worker declares `mod spawn_join;` in `src/commands/usage/mod.rs` and calls your API; do not edit that file.

## Interface (pinned, write it exactly)

```rust
use std::path::Path;

pub(crate) const AMBIGUOUS_MODEL: &str = "ambiguous";

pub(crate) struct RequestedModels { /* private map: (parent_session_id, agent_id) -> String */ }

impl RequestedModels {
    pub(crate) fn load(work_dir: Option<&Path>) -> Self;
    pub(crate) fn requested_model(&self, agent_id: &str, parent_session_id: &str) -> Option<&str>;
}
```

## Steps

1. `load`: `None` gives an empty value. For every plain (non-symlink) directory under `<work_dir>/subagents/`, read `spawns.jsonl` and `starts.jsonl` line by line, skipping lines that are not JSON objects. A spawns row carries `ts`, `session_id` (the loom session), `agent_type` and `model` (an empty string means no model). A starts row carries `agent_id`, `agent_type`, `parent_session_id`, `loom_session_id` and `ts`. Read at most 8 MiB per file.
2. Group spawn rows by `(session_id, agent_type)` and start rows by `(loom_session_id, agent_type)`, each group in file order, then stable-sorted by `ts` (the strings share one format, so they sort as text). For each start group with a matching spawn group: when the distinct non-empty models of the spawn group number exactly one, every start gets it. Otherwise, when both groups have the same length, the i-th start gets the i-th spawn model, except that it gets `AMBIGUOUS_MODEL` when another spawn of the group has the same `ts` and a different model (two parallel same-type spawns). Otherwise every start of the group gets `AMBIGUOUS_MODEL`. Starts without a matching spawn group get nothing.
3. `requested_model` looks up `(parent_session_id, agent_id)`.

## Tests (`spawn_join_tests.rs`, declared from the new file with `#[cfg(test)] #[path = "spawn_join_tests.rs"] mod tests;`)

Write the two JSONL files into a `tempfile::tempdir()` work dir under `subagents/stage-a/`:

- one spawn and one start of `loom-software-engineer` with model `sonnet` gives `Some("sonnet")`;
- two spawns of one type at the same `ts` with models `sonnet` and `opus` and two starts give `Some("ambiguous")` for both agents;
- two spawns of one type at different `ts` with models `haiku` then `sonnet` and two starts in `ts` order give `haiku` to the first agent and `sonnet` to the second;
- a start whose loom session has no spawns gives `None`; `load(None)` gives `None` for every lookup.

## Done means

Your two files compile against the interfaces pinned in common.md. Run no build, test or formatter: the Claude workers build the same crate in this worktree at the same time (one of them adds `mod spawn_join;`, so your file is outside the module tree until then), a cargo command can wait on their build lock past the 540-second forward deadline, and the orchestrator compiles right after you. If you meet a file you do not own that does not fit the pinned interfaces, report it and do not touch that file. The orchestrator runs `cargo test --lib commands::usage::spawn_join::` after you return. File under 250 lines, every function under 50 lines, after `cargo fmt` (keep a margin: fmt grows code). No git, no path under `.loom/`. Doc comments put any placeholder such as `<work_dir>` inside backticks.
