# Ledger Tui Rendering

> Topic notes for the mistakes knowledge area.

## Ledger TUI: Wide Glyphs, Fan-Out Duplication, and Latent Panics (2026-09-04)

**East-Asian Wide glyphs break `chars().count()` padding.** `⚡` (U+26A1, the `MergeConflict` icon)
is East-Asian Wide: one `char`, two terminal cells. `ledger/cells.rs` originally padded by
`chars().count()`, so a `MergeConflict` row shifted every later column by one cell and clipped the
MERGE cell; `ledger/header.rs` already used `Span::width()` (unicode-width aware) as `text_width()`.
**Prevention:** any TUI cell padding in this repo measures with `Span::width()`, never
`chars().count()` — the status icon set contains at least one wide glyph.

**Fanning a UI out by file reproduces the same defect in every worker's slice.** Splitting the ledger
across five parallel workers, each owning a disjoint file set, produced the SAME defect class three
times independently: a `chars().count()` pad in `columns.rs`, a `{:<20}` format pad on a stage id in
`panels.rs`, a `{:<12}` pad on a status label in `legend.rs`, plus three byte-identical copies of a
`cut_line`/`spans_width` truncation loop across `header.rs`, `panels.rs`, and `legend.rs`. No worker
could see a sibling's shared helper while writing. **Prevention:** when a plan fans UI work out by
file, give the foundation module (the one worker who runs first, alone) the width/truncation helpers
up front and name them explicitly in every other worker's brief; otherwise budget an orchestrator
pass afterward to converge the duplicates.

**A design table's own example content did not fit its own column width.** The MODELS column is 16
cells, pinned by `FULL_WIDTH=120` (`ledger/mod.rs:40`) and the drop-order termination proof at width
64 — it cannot widen. The plan's design table and a worker brief both used `opus›sonnet,terra` (17
cells) as example acceptance content, which the column's own truncation rule cannot render intact
(the real output is `opus›sonnet+1`). **Prevention:** when a design table gives both a column width
and example content, check the example actually fits before writing it into an acceptance criterion.

**Two latent panics/overflows shipped with no test reaching them.** `ledger/header.rs` collects
`crate::LOGO.lines()` into a `Vec` and indexes `logos[0..3]`; `LOGO` (`lib.rs:36`) happens to have
exactly four lines today, so shortening the banner would panic the dashboard at render time — prefer
`logos.get(n).copied().unwrap_or("")`. And `ui/tui/state.rs` did `self.scroll_y + delta as u16` while
the sibling negative-delta branch two lines above correctly saturates; only reachable above ~32768
stages, so hardening rather than a live bug. **Prevention:** when one branch of a numeric pair
saturates and its sibling does not, that asymmetry is the bug even when the overflow is unreachable
today — search for the sibling branch, don't just fix the one that was reported.
