# token-core W1: wire types, effort accounting, flow build, sanitize, fixture and schema

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/briefs/live-token-insights/common.md` first; its sections 2, 3 and 5 are your spec. Paths below are relative to `loom/` unless they start with `../web/`.

You own: `src/tokens/{model,claude_line,codex_line,accumulate,checkpoint,flow_build}.rs` and their `_tests.rs`, `src/tokens/tests_fixture.rs`, `src/commands/status/data/sanitize_token_flow.rs` and `_tests.rs`, `src/commands/status/web/model_tests.rs`, the three TUI test files named in step 7, `tests/token_flow_contracts.rs`, `../web/src/api/schema.ts`, `../web/src/api/schema.test.ts`, `../web/src/api/fixtures/snapshot.json`, `../web/src/lib/tokens.test.ts`. A parallel worker (W2) owns `src/tokens/{mod,tracker,pass_memo,quota_points}.rs`; do not edit those. Never touch `tests/token_insights_contracts.rs` or anything under `tests/token_insights_contracts/` (frozen contracts).

## Steps

1. **Model** (`src/tokens/model.rs`): add `ModelUsage`, `StageFlow.hit_series` (after `series`), `TokenFlow.models` (after `stages`) and the four constants exactly as common.md section 2 gives them, with doc comments. `pub use model::*` in `mod.rs` already re-exports them.
2. **Parsers.**
   - `claude_line.rs`: `ClaudeLine::Usage` gains `effort: Option<String>`, read from the line's top-level `perTurnEffort` string, else top-level `effort` string, else `None` (a non-string value counts as absent). Tests: perTurnEffort wins over effort; effort alone; neither; a numeric `perTurnEffort` is absent.
   - `codex_line.rs`: replace `CodexLine::Model(String)` with `CodexLine::TurnContext { model: String, effort: Option<String> }`; a `turn_context` without a string `payload.model` stays `Other` as today. Update its tests (the `Model(...)` assertions become `TurnContext { .. }` with the effort they carry).
3. **Accumulation** (`accumulate.rs`):
   - `AgentState` gains `effort: Option<String>` and `by_model: Vec<ModelUsage>` (both serialized in the checkpoint, no `serde(default)`).
   - `ingest_claude`: set `self.effort = Some(inline_safe(&e))` when the line has an effort (as model is set today, after the synthetic filter for model).
   - `ingest_codex`: `TurnContext { model, effort }` sets `self.model` and, when present, `self.effort`, both through `inline_safe`.
   - `add` books the same `usage` into `by_model` under the key `(self.model or UNKNOWN_MODEL, self.effort)` through a new private `fn add_model_usage(&mut self, usage: &TokenUsage)`: find the entry with that model and effort and add; else, when fewer than `MAX_MODEL_KEYS` entries other than `(OTHER_MODEL, None)` exist, push a new entry; else add to the `(OTHER_MODEL, None)` entry, creating it if needed. `add` already returns early on an all-zero usage; keep that.
   - `restart` already rebuilds through `Self::new`, which must start `effort: None` and `by_model: Vec::new()`.
   - Tests in `accumulate_tests.rs`: per-line effort split (two messages, two efforts); an effort-less line after an effort line inherits it; the cap (10 models → 8 keys plus other, sums equal total); a Codex turn_context effort; restart clears `by_model`.
4. **Checkpoint** (`checkpoint.rs`): `CHECKPOINT_VERSION` = 2. Add a test that a version-1 file is refused with the existing "checkpoint is version 1" reason.
5. **Flow build** (`flow_build.rs`):
   - `fn model_totals(stages: &BTreeMap<String, StageAgents>) -> Vec<ModelUsage>`: merge every agent's `by_model` by (model, effort), sort and fold to `MAX_MODEL_ROWS` exactly as common.md section 2 says (merge an existing `("other", None)` entry with the fold). `build_flow` sets `models: model_totals(stages)`.
   - `fn hit_series(buckets: &Buckets, now_bucket: i64, len: usize) -> Vec<Option<u8>>` per common.md section 2; `stage_flow` sets `hit_series: hit_series(&buckets, now_bucket, STAGE_SERIES_LEN)`.
   - `rate(buckets, now_bucket)` becomes `rate(buckets, now, now_bucket)` with the span formula of common.md section 3, `saturating_mul` before the division. Update all three call sites.
   - In `flow_build_tests.rs` change `const NOW: i64 = 1_788_523_200;` to `1_788_523_229` (same bucket, last second; span 300, so every existing rate assertion keeps its value). Add tests: rate at the first second of a bucket uses span 271; models sorted and summing to plan total; the `MAX_MODEL_ROWS` fold; hit_series rounding (75, 25), an empty bucket `None`, a value of 100.
6. **Sanitize** (`src/commands/status/data/sanitize_token_flow.rs`): for each `flow.models` entry `flatten` `model` and the `effort` if any, `clamp_usage` its total; truncate `flow.models` to `MAX_MODEL_ROWS`; for each stage bound `hit_series` to its newest `STAGE_SERIES_LEN` values and clamp each `Some(v)` to at most 100. Tests in `sanitize_token_flow_tests.rs` for each rule.
7. **Literal sites** (each gains `hit_series` on `StageFlow` and `models` on `TokenFlow`; use `Vec::new()` unless the test needs values): `src/tokens/model_tests.rs:11,19`, `src/commands/status/data/sanitize_token_flow_tests.rs:26,34`, `src/commands/status/ui/tui/ledger/tokens_pane_tests.rs:60,68`, `src/commands/status/ui/tui/ledger/tests_tokens.rs:88,101`, `src/commands/status/ui/tui/ledger/tests_tokens_contracts.rs:84,92`, and `src/commands/status/web/model_tests.rs:132,140` (this one gets the fixture values, step 8). Add only the field lines; change no assertion line.
8. **Fixture pair.** In `../web/src/api/fixtures/snapshot.json` add `"hit_series"` (120 values of 94) after the server stage's `"series"`, and `"models"` (the four entries of common.md section 5, in that order, each `{"model","effort","total":{"input","cache_write","cache_read","output"}}`) after `"stages"`. Mirror both in `fixture_snapshot()` of `src/commands/status/web/model_tests.rs`. `cargo test --lib commands::status::web::` must pass (`fixture_matches_serde_output`).
9. **zod** (`../web/src/api/schema.ts`): common.md section 2. In `../web/src/api/schema.test.ts` add: an unknown key on a models entry is rejected; a hit value of 101 is rejected; `null` hit values parse. In `../web/src/lib/tokens.test.ts` add `hit_series` to the StageFlow literals (around lines 52 and 149) and `models: []` to any TokenFlow literal, so `bunx tsc -b --noEmit` passes. Run `bunx oxfmt` on the four web files you changed.
10. **Old rate contract.** In `tests/token_flow_contracts.rs` `rate_is_five_minute_average`, change only `let now = unix_now();` to the last second of the previous bucket: `let now = unix_now().div_euclid(BUCKET_SECS) * BUCKET_SECS - 1;` (import `BUCKET_SECS` if it is not yet in scope). Its usage lines sit at `now - 420` and `now - 60`, so the 3000 stays in the window and the assertion of 600 holds with a 300 s span.

## Proof

From `loom/`: `cargo test --lib tokens::`, `cargo test --lib commands::status::`, `cargo test --test token_flow_contracts`, `cargo clippy --all-targets -- -D warnings`. From `web/`: `bunx tsc -b --noEmit`, `bunx vitest run src/api src/lib`. Report the counts. W2 edits `tracker.rs` at the same time; a compile error inside W2's files is theirs, so report it rather than fix it.

## Traps

- `inline_safe` is `crate::context::untrusted::inline_safe`, already imported in `accumulate.rs`.
- Keep every function under 50 lines and every file under 400; `accumulate.rs` is 262 lines and `flow_build.rs` 183 today.
- `sanitize_token_flow` uses `flatten` from the parent module (`super::flatten`).
- The JSON fixture must stay oxfmt-clean: arrays are packed several values per line as the existing `"series"` arrays are (`snapshot.json:409`); run `bunx oxfmt src/api/fixtures/snapshot.json` from `web/` and let it lay the new arrays out.
- `tests/token_flow_contracts.rs` imports from `loom::tokens::{...}` at line 7; add `BUCKET_SECS` there if it is missing.
