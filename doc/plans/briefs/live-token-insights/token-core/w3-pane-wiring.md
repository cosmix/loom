# token-core W3: wire the by-model block into the t pane

Tier: haiku (`loom-software-engineer`, model haiku). Read `doc/plans/briefs/live-token-insights/common.md` section 1 first. Paths are relative to `loom/`.

You own: `src/commands/status/ui/tui/ledger/tokens_pane.rs` and new `src/commands/status/ui/tui/ledger/tokens_pane_models_tests.rs`. Read but do not edit `tokens_models.rs` (C2 wrote `model_lines(models: &[ModelUsage], width: u16) -> Vec<Line<'static>>` and `MODEL_BLOCK_MAX_ROWS`), `layout.rs`, and `tokens_pane_tests.rs` (374 lines; leave it alone).

## Today

`tokens_pane.rs:12` `pub(super) const PANE_MAX_ROWS: u16 = 16;`. `tokens_pane_lines(flow, width, max_rows)` (`:159-190`): returns nothing for `max_rows == 0`; a single title line noting "no active agent" when no stage has an active agent; else the title plus stage and agent lines until `max_rows`. `layout.rs:43-56` calls it with `PANE_MAX_ROWS` and budgets `pane.len()` rows.

## Steps

1. Add `const LIVE_MAX_ROWS: usize = 16;` and set `PANE_MAX_ROWS` to `16 + 1 + MODEL_BLOCK_MAX_ROWS` written as `(LIVE_MAX_ROWS + 1 + MODEL_BLOCK_MAX_ROWS) as u16`.
2. In `tokens_pane_lines`: the live part (title, stage and agent lines) stops at `LIVE_MAX_ROWS` lines instead of `max_rows`; the "no active agent" title stays as the live part when nothing is active. Then, when `model_lines(&flow.models, width)` is not empty, append one empty line and its lines. Finally truncate the whole vec to `max_rows`. Keep the function under 50 lines (move the live part into a helper if needed).
3. Import `model_lines` and `MODEL_BLOCK_MAX_ROWS` with `use super::tokens_models::{model_lines, MODEL_BLOCK_MAX_ROWS};`.
4. Tests in `tokens_pane_models_tests.rs`, declared from `tokens_pane.rs` with `#[cfg(test)] #[path = "tokens_pane_models_tests.rs"] mod models_tests;` beside the existing `mod tests;`: with an active agent and the four models of common.md section 5, a line after the live rows contains `BY MODEL` and a later one `claude-sonnet-5-5`; with no active agent the pane still shows `no active agent` and then the block; with `models` empty the output equals today's (same line count as without the block); `max_rows` smaller than the total cuts from the bottom. Build the `TokenFlow` with the same literal shape `tokens_pane_tests.rs:60-75` uses (copy, do not import its private helpers).

## Proof

`cargo test --lib commands::status::ui::tui::ledger::`, `cargo clippy --all-targets -- -D warnings` (no dead-code warning for `model_lines` once you call it), `cargo fmt --check`.
