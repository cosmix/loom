# TUI C1: token pane renderer and text helpers (codex unit)

Read doc/plans/briefs/token-flow/common.md sections 2, 3 and 6 first. Stage token-core has merged, so crate::tokens::{TokenFlow, StageFlow, AgentFlow, AgentKind, QuotaLeft} and StatusData.tokens exist. Do not run git. Do not touch any path under .loom.

## Files

- Write: loom/src/commands/status/ui/tui/ledger/tokens_pane.rs and loom/src/commands/status/ui/tui/ledger/tokens_pane_tests.rs.
- Read only: loom/src/commands/status/ui/tui/ledger/{mod.rs, text.rs, cells.rs, panels.rs} (styles, padding helpers, the panel title style).
- The orchestrator adds `mod tokens_pane;` to ledger/mod.rs before you start. Do not edit mod.rs.

## Surface (pinned; worker W1 of this stage calls it)

    pub(super) const PANE_MAX_ROWS: u16 = 16;
    pub(super) fn compact_tokens(n: u64) -> String
    pub(super) fn spark(values: &[u64], cells: usize) -> String
    pub(super) fn shade_strip(values: &[u64], max: u64) -> String
    pub(super) fn quota_left_text(left: &QuotaLeft) -> Option<(String, Style)>
    pub(super) fn tokens_pane_lines(flow: &TokenFlow, width: u16, max_rows: u16) -> Vec<Line<'static>>

## Steps

1. Text helpers.
   - compact_tokens: under 1000 gives the plain number ("980"); under 100,000 gives one decimal plus k, truncated, never rounded ("21.8k"; 99,999 gives "99.9k"); under 1e6 gives whole k, truncated ("312k"; 999,999 gives "999k"); otherwise two decimals plus M, truncated ("2.23M"). Truncation keeps every result at most 5 cells, the width of the rate slot (rounding would give "100.0k").
   - spark: the last `cells` values mapped onto the block characters U+2581 to U+2588 against their own maximum. A zero value is a space; the result has exactly `cells` chars, left-padded with spaces when there are fewer values.
   - shade_strip: each value mapped onto space, U+2591, U+2592, U+2593, U+2588 against `max` (zero is a space).
   - quota_left_text:
     - lasts_to_reset gives ("lasts to reset", dim);
     - Some(m) with m >= 60 gives ("~{h}h{mm}m left") with mm zero-padded, and m < 60 gives ("~{m}m left");
     - the style is red when m < 30, else yellow;
     - None with lasts_to_reset false gives None.
2. tokens_pane_lines renders at most max_rows lines.
   - Title line: " TOKENS" in the panel title style, then dim " \u{b7} fresh per 30 s cell \u{b7} \u{2591}\u{2592}\u{2593}\u{2588} low to high", then right-aligned dim "t close".
   - For each stage in flow.stages with any active agent: a stage line with the stage id bold, its fresh rate as compact_tokens(rate.fresh()) + "/m", and spark(series, 48).
   - One line per agent, starting with exactly two spaces (the contract finds the line whose first non-space text is "main"):
     - label: "main" for kind main; agent_type with a leading "loom-" removed, plus a space and the first 4 chars of id, for a subagent; "codex " plus the first 4 chars after the codex- prefix for codex;
     - model, dim, in magenta for codex;
     - "live" in blue when active, else dim "idle";
     - shade_strip over the last 48 series values against the stage maximum, left-padded with spaces to 48 cells when the series is shorter (an idle agent's series can be empty, common.md section 3);
     - the agent fresh total, right-aligned.
   - Truncate every line to width with the existing helpers (never by byte slicing). Stop at max_rows.
3. Unit tests in tokens_pane_tests.rs, declared with `#[cfg(test)] #[path = "tokens_pane_tests.rs"] mod tests;`:
   - compact_tokens at 0, 999, 1000, 21800, 99999, 312000, 999999 and 2230000;
   - spark length and the zero-is-space rule;
   - quota_left_text for 127, 43, 20, lasts_to_reset, and None;
   - a pane built from the common.md section 4 fixture values has a title, one stage line and three agent lines, and fits width 132;
   - max_rows 2 yields 2 lines;
   - a width of 64 never panics and no line exceeds it.

Done when your files compile and your tests are written. Your only check is `cargo test --lib --no-run` from loom/ (TMPDIR is read-only inside the codex sandbox); the orchestrator runs `cargo test --lib commands::status::ui::tui::ledger::tokens_pane` after you return. The orchestrator seeded tokens_pane.rs with one `//!` line; replace it. Keep both files under 300 lines, functions under 50 lines, rustfmt clean.
