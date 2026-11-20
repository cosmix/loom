# TUI W1: TOKENS column, header summary, quota time left, `t` pane wiring (Claude, sonnet)

Read `doc/plans/briefs/token-flow/common.md` first, then this brief. Unit C1 has already landed `ledger/tokens_pane.rs` with `compact_tokens`, `spark`, `shade_strip`, `quota_left_text`, `tokens_pane_lines` and `PANE_MAX_ROWS` (signatures in `c1-pane.md`). Use them and do not edit that file. All paths below are under `loom/src/commands/status/ui/tui/`.

## Files you own

- `ledger/mod.rs`
- `ledger/columns.rs`
- `ledger/cells.rs`
- `ledger/rows.rs`
- `ledger/header.rs`
- `ledger/layout_tokens.rs` (new)
- `ledger/quota.rs`, `ledger/quota_tests.rs`
- `ledger/panels.rs`, `ledger/footer.rs` (new, only if needed; step 5)
- `ledger/layout.rs`
- `ledger/legend.rs`
- `ledger/tests.rs`, `ledger/tests_tokens.rs` (new), `ledger/tests_alignment.rs`, `ledger/tests_viewport.rs`
- `event_handler.rs`
- `app.rs`

Read-only and frozen: `ledger/tests_tokens_contracts.rs` and `ledger/tests_completion.rs`. The contracts call `screen`, `contains`, `make_stage` and `fixture` in `ledger/tests.rs`: never change those four signatures. The orchestrator gives you the current line counts of `layout.rs`, `app.rs`, `panels.rs` and `tests.rs`; the limit is 400 lines per file and 50 per function (`cargo test --test maintainability`).

## Changes

1. **Column** (`mod.rs`, `columns.rs`):
   - Add `ColumnKind::Tokens`, header `TOKENS`, width 10, gap 2, placed after Context.
   - `FULL_WIDTH` becomes 132.
   - The drop list in `columns_for_width` becomes `[Time, Models, DependsOn, Tokens, Context]`.
   - With these numbers, full width is 132. Dropping Time gives 123, then Models 105, DependsOn 87, Tokens 75 and Context 64. So TOKENS shows at width 90 and is gone at 80.
   - Update every width-pinned test in `columns.rs`, `tests_alignment.rs`, `tests.rs` and `rows.rs` to the new arithmetic: `rows.rs`'s test that renders `stage_row` at 120 and asserts width 120 (it becomes 105 without TIME and MODELS), and the MODELS-at-120 assertions in `tests.rs` and `tests_alignment.rs` (they move to 132). Change numbers, never weaken an assertion. At 120-131 the ledger now drops TIME and MODELS (plan Decision 15).
2. **Cell** (`cells.rs`): a `tokens_cell` arm in `cell_for`.
   - For an executing stage with a `StageFlow`: `spark(series, 4)`, a space, then `compact_tokens(rate.fresh())` right-aligned in 5 cells.
   - For a completed stage with a flow: `compact_tokens(total.fresh())` right-aligned, dim.
   - Otherwise empty.
   - `cell_for` must reach the flow. Today `stage_row(stage, level, cols, all)` (`rows.rs`) and `cell_for(stage, level, column, all)` (`cells.rs`) take no `StatusData`; the flow is reachable only as `view.data.tokens` in `layout.rs` `render_table`. There, look up the stage's `StageFlow` by id and pass `Option<&StageFlow>` down through `stage_row` to `cell_for`. Contract `tokens-cell-shows-stage-rate` checks the executing row shows "43.2k".
3. **Header row 3** (`header.rs` `merge_line`):
   - When `data.tokens` is `Some`, append three spaces then `tokens {compact rate}/m {spark(plan.series, 12)} · {compact total} total · hit {pct}%` after the merge text.
   - Dim labels, bold rate, yellow spark. `pct` uses common.md's cache-hit definition with one decimal, or "—".
   - Add it only when it and the version label both fit; drop the token text before the version label.
   - The budget at 132 is tight: about 113 cells, of which the token text takes 56 and `fixture()`'s merge text 35, leaving 21 for the version label plus a gap. A worktree build's label is `v<ver>-dev.N+<8 sha>` (`version/derive.rs`), 21 cells for N ≤ 9 and 22 from N = 10, so a test of the summary at 132 would flip with the commit count. Test the summary at width 150 or more, and test the drop at a narrow width. Contract `header-shows-plan-token-summary` expects one row holding "tokens 43.2k/m", "2.23M total" and "hit 93.1%" at 150x40.
4. **Quota time left** (`quota.rs`):
   - Add `const QUOTA_FULL_WIDTH: u16 = 120;` and use it in `quota_layout`, so the quota tiers do not move when `FULL_WIDTH` becomes 132.
   - `quota_line` takes `left: &[QuotaLeft]` as a new second parameter.
   - After a provider's 5h window text, append a space and `quota_left_text` for that provider's entry, in the FULL and MEDIUM layouts only.
   - Update the callers: the only production caller is `render_footer` in `ledger/panels.rs`, which passes `&view.data.quota` and `view.now_epoch`; it now also passes `view.data.tokens.as_ref().map_or(&[][..], |f| &f.quota_left)`. Then `quota_tests.rs`. Add tests for "~2h07m left", "lasts to reset", no entry, and the NARROW layout showing no suffix. Contract `footer-quota-shows-time-left` renders the whole ledger and checks the footer's "claude 5h" row.
5. **Pane**:
   - `TuiApp` gains `show_tokens: bool` (`app.rs`, default false), and `LedgerView` gains `show_tokens`.
   - `event_handler.rs`: `KeyCode::Char('t')` maps to a new `KeyEventResult::ToggleTokens`, which `app.rs` handles by toggling.
   - `layout.rs`: when `show_tokens` is set and `data.tokens` is `Some`, render `tokens_pane_lines(flow, width, rows)` between the table and the attention panel. Add `tokens` to `Budget` and `Areas`.
   - The pane is the first thing `budget()` drops, before activity, when the table would fall under `TABLE_MIN_HEIGHT`. Cap it at `PANE_MAX_ROWS`.
   - The key hints gain `t tokens` (bold `t`, then a dim span holding a space and `tokens`) between legend and scroll. They are `key_hints` in `ledger/panels.rs`, and a `panels.rs` test pins `"? legend · q quit"` at the end of the strip: update that assertion to the new strip, never delete it. If `panels.rs` would pass 390 lines, move `render_footer`, `footer_line`, `key_hints` and their helpers and tests into a new `ledger/footer.rs` (declare it in `mod.rs`).
   - `layout.rs` is at 377 of 400 lines. Move whatever you need out (for example the pane area arithmetic) into a new `ledger/layout_tokens.rs`, so `layout.rs` stays under 380 lines after `cargo fmt`. `app.rs` is at 390; keep it at or under 398.
6. **Legend** (`legend.rs`): the overlay's inner width is 76 cells, so wrap the new text:
   - After the context line, add three lines, matching the indentation of the existing context entry (2 spaces for the term, 4 for its text): `tokens`, then `fresh tokens per minute (input + cache writes + output),`, then `5 min average; t opens the per-agent pane`. Each line fits 76 cells.
   - Raise the overlay height cap from 22 to 24.
7. **StatusData literals:** stage token-core already added `tokens: None` to the test literals. Use `Some(..)` built from the common.md section 4 values wherever your tests need a flow, through one shared test helper in the new `#[cfg(test)]` `ledger/tests_tokens.rs` (declared in `mod.rs` beside `tests`), not in `ledger/tests.rs`, which is at 363 lines. Build the three `AgentFlow`s with a small helper that takes a struct or fewer than 7 arguments: an 8-argument builder fails `clippy::too_many_arguments` under `-D warnings`.

## Check

`cargo test --lib commands::status::ui::tui::`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and `cargo test --test maintainability`, all from `loom/`.
