# token-core C2: TUI by-model block renderer (codex unit)

Files you write: `loom/src/commands/status/ui/tui/ledger/tokens_models.rs` (it exists with one `//!` doc line; keep that line) and new `loom/src/commands/status/ui/tui/ledger/tokens_models_tests.rs`. Read anything else; write nothing else. Do not run git. Do not touch any `.loom` path. Your only check is `cargo test --lib --no-run` from `loom/`.

Anchors: `ModelUsage`, `TokenUsage` in `loom/src/tokens/model.rs` (`ModelUsage { model: String, effort: Option<String>, total: TokenUsage }`, `TokenUsage::fresh()`); the pane's helpers and styles in `loom/src/commands/status/ui/tui/ledger/tokens_pane.rs` (`compact_tokens` is `pub(super)` at line 30; `title_line` and the dim/plain styles used by `agent_line`); the plan's shared spec `doc/plans/briefs/live-token-insights/common.md` section 5 (the four-entry example).

## Steps

1. Add to `tokens_models.rs`:
   - `pub(super) const MODEL_BLOCK_MAX_ROWS: usize = 10;`
   - `pub(super) fn model_lines(models: &[ModelUsage], width: u16) -> Vec<Line<'static>>`. Empty `models`, or a fresh sum of 0: return an empty vec. Otherwise group the entries by `model`, summing `total` over its efforts; order the models by fresh descending, then name ascending; inside a model keep its efforts in the order of `models`. Output, truncated to `MODEL_BLOCK_MAX_ROWS` lines from the bottom:
     - title line, dim: ` BY MODEL · fresh so far · share`
     - per model: two spaces, the model name left-aligned in a column of `min(24, longest name)` cells (truncate with `…` when longer), two spaces, a bar of `BAR_CELLS` (20) cells where `round(share * 20)` cells are `█` and the rest `░`, two spaces, the share as a whole percent right-aligned in 4 cells (`50%`), two spaces, `compact_tokens(fresh)` right-aligned in 7 cells.
     - per effort of that model: six spaces, the effort or `—` left-aligned in 8 cells, two spaces, `compact_tokens(fresh)` then ` fresh · `, then `compact_tokens(cache_read)` then ` cache read`, dim.
     - share = model fresh / sum of all fresh; percent rounds half up.
   - Below a width of 60 cells, drop the bar (keep name, percent, fresh). Never emit a line wider than `width` cells.
   - `#[cfg(test)] #[path = "tokens_models_tests.rs"] mod tests;` at the end.
2. Tests in `tokens_models_tests.rs`, using the four entries of common.md section 5 (sonnet medium 50000/900000/13000000/160000; opus xhigh 30000/450000/7000000/80000; gpt-6.1-sol xhigh 30000/300000/4000000/50000; opus high 10000/150000/2000000/20000): the second line contains `claude-sonnet-5-5`, `50%` and `1.11M`; the opus line contains `33%` and `740k`; its effort lines follow it and contain `xhigh`, `560k`, `7.00M` and `high`, `180k`; empty input gives no lines; width 50 has no `█`; no line exceeds the width at widths 50, 80 and 132; more than 10 lines are cut to 10.
3. Keep every function under 50 lines and the file under 300 lines (a later `cargo fmt` adds lines). Run `cargo test --lib --no-run`.

Constraints: `model_lines` is unused until another worker calls it from `tokens_pane.rs`, so the compiler's dead-code warning is expected; do not add `#[allow(dead_code)]`. Literal braces inside `format!` must be doubled. Widths are display cells: use the width helper `tokens_pane.rs` uses for its padding (look it up with `loom map --outline loom/src/commands/status/ui/tui/ledger/tokens_pane.rs`).
