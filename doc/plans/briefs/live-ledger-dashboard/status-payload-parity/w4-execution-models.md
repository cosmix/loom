# W4 — `execution_models_for_stage`: which models actually did the work

Tier: codex `gpt-5.6-luna`, effort `xhigh`. Runs in the first wave, in parallel with W1 and W5.

## File you own (write)

- `loom/src/commands/status/data/execution_models.rs` — new, with an inline `#[cfg(test)] mod tests`.

Read-only: `hooks/spawn-guard.sh` (function `record_spawn`, the JSON row shape),
`loom/src/commands/subagents/table.rs` (`display_model`, the narrowing to replicate),
`loom/src/fs/work_dir.rs` (`WorkDir::root`).

W1 declares `mod execution_models;` and re-exports your function from `data/mod.rs`; the crate
compiles only once both files exist. Do not edit `data/mod.rs`.

## Signature (W1 compiles against this exactly)

```text
pub fn execution_models_for_stage(work_dir: &WorkDir, stage_id: &str) -> Vec<String>
pub fn display_model(raw: &str) -> String
```

## Sources

Directory: `work_dir.root().join("subagents").join(stage_id)`. Return an empty vector when
`stage_id` contains anything outside `[A-Za-z0-9._-]`, when the directory is missing, or on any
read error. Never return an error; skip malformed lines.

1. `spawns.jsonl` — one JSON object per line, written by `record_spawn` in `hooks/spawn-guard.sh`:
   `{ts, stage_id, session_id, caller, agent_type, model, model_source, description}`. Take `model`
   when non-empty, EXCEPT when `agent_type` is `loom-codex-forwarder` or `codex:codex-rescue` — for
   those rows `model` is the shim's own tier, not the model that ran the task.
2. `codex.jsonl` — one JSON object per line, written by `hooks/codex-forward.sh` (W5 adds it):
   `{ts, stage_id, session_id, model, effort, mode}`. Take `model`.

Read `spawns.jsonl` first, then `codex.jsonl`; map each raw value through `display_model`; keep the
first occurrence of each display name (order preserved); drop empty results.

## `display_model`

Replicate the narrowing `display_model` in `commands/subagents/table.rs` performs (that function
is private; do not import it): strip a leading `claude-`; strip a trailing `-YYYYMMDD` (a dash
followed by exactly eight digits at the end). Then additionally strip a leading `gpt-5.6-`. Tier
names (`sonnet`, `opus`, `haiku`, `fable`) pass through unchanged.

Examples: `sonnet` → `sonnet`; `claude-opus-5` → `opus-5`; `claude-sonnet-4-5-20250929` →
`sonnet-4-5`; `gpt-5.6-terra` → `terra`; `gpt-5.6-sol` → `sol`.

## Tests (inline)

- `missing_directory_is_empty`
- `unsafe_stage_id_is_empty` (`../x`)
- `spawn_rows_yield_distinct_models_in_order` (rows sonnet, opus, sonnet → `["sonnet","opus"]`)
- `forwarder_rows_are_skipped_and_codex_rows_counted` (a `loom-codex-forwarder` row with model
  `sonnet` plus a `codex.jsonl` row `gpt-5.6-terra` → `["terra"]`)
- `malformed_lines_are_skipped`
- `display_model_narrows_ids` (the five examples above)

Build the fixture with `tempfile::TempDir` and a `WorkDir` rooted in it (see how
`collector_tests.rs` constructs one).

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::data::execution_models::`
passes once W1's edits exist. Keep the file under 400 lines and every function under 50.

## Constraints the graph will not show you

- Use `serde_json::Value` line parsing; no new dependencies.
- Never read `.work/admin.token` or `.work/user.token`; never grep across `.work/`.
- Do not run `git`. Do not run the full test suite, the linter or the formatter.
