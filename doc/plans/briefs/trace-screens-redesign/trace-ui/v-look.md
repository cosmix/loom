# V1 and V2: the look (trace-ui, wave 2)

Invoke the skill `frontend-design:frontend-design` before anything else. Then read `../design.md` in
full, `web/src/plan/ui/DESIGN.md`, and the W1, W2 and W3 reports your orchestrator passes you.

The target is the approved screenshots in `doc/plans/mockups/trace-screens-redesign/`. Open every
screenshot named for your screens and match their craft: hierarchy, rhythm, reading-size type,
alignment, and the warmth of the Ledger and Walnut themes. The user rejected two earlier rounds as
bland and cramped. Where a screenshot uses a serif for a title or heading, use Inter instead
(design.md, "The user's change after review").

## Territories

| Worker | Screens | Files owned |
| --- | --- | --- |
| V1 | spine, document heading, plan page, opened passage line, turn footer block | `web/src/plan/trace/trace.css` (split into `trace-heading.css` and `trace-spine.css` if it passes 400 lines), `web/src/plan/feature/spine.css`, `web/src/plan/ui/doc-heading.css`, `web/src/plan/ui/DESIGN.md` |
| V2 | Coverage and Findings pages, the two entry links | `web/src/plan/trace/maps.css`, `maps-columns.css` (delete if empty), `maps-findings.css` |

You may change class names and markup in the TSX files the W reports name only through a list you
return. The orchestrator applies it or hands it to a fresh worker. V2 returns its DESIGN.md lines
(one entry each for Coverage and Findings) for V1's orchestrator to apply; V1 writes the Type step,
the spine Status sign rule (the roundel in a running plan row), and the settle glyph and amber
attention rules.

## Rules

- Use existing tokens only: tones, document hues, `--font-sans`, `--font-reading` and
  `--font-mono`. No colour literals, and never hard-code Literata.
- Content sizes follow design.md "Look". Nothing in content is below 13px.
- Layout safety: `min-width: 0` on tracks, ellipsis on long text, and no sideways scroll at 390px.
- Delete every CSS rule whose markup the W workers removed (their reports list them).

## Verify in Firefox

Build first, from the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`. Then:

```bash
scripts/plan-screens.sh --worktree-doc --browser=firefox loom/target/debug/loom "$LOOM_SCRATCH_DIR/<you>" <routes>
```

The command shoots 1440 and 390 in Ledger and Walnut. Routes are against the repository's own
`feat-planner` feature:
- V1:
  - `/f/feat-planner/product`
  - a product section summary page
  - a plan summary page
  - `/f/feat-planner/source/<a section path>`
- V2: `/f/feat-planner/coverage`, `/f/feat-planner/findings`, `/f/feat-planner/run`

Put each of your shots beside the matching approved screenshot and fix what differs, until each
screen would pass for the approved one, apart from the Inter titles. If `--browser=firefox` does not
launch (W1's report says), say so and shoot Chromium. Report that as a gap; do not count it as
done.

## Report

- the files changed and the shot paths
- for each approved screenshot, one line naming any remaining difference
- the markup change list for the orchestrator
