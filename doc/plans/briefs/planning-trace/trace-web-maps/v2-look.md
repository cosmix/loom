# V2: the look of the Coverage and Findings pages (trace-web-maps, wave 2)

Invoke `frontend-design:frontend-design` first. Read `../common.md` "The browser",
`web/src/plan/ui/DESIGN.md` in full (including V1's entries), and W2's report. You own
`web/src/plan/trace/maps.css`, `web/src/plan/help/**` and `web/src/plan/ui/DESIGN.md`; markup
changes in W2's files go back to the orchestrator as a list.

## What to design

- **Coverage page.** A wide layout that uses the page width (it is a map, not a reading column):
  five labelled columns, compact cards in the planner's card style, a lineage highlight that
  dims unrelated cards and marks related ones with the accent, gap cards with a visible marker
  and text, not colour alone. The focus panel docks on the right at 1280px and up and becomes a
  bottom sheet below. At 390px the columns become a vertical list of sections, each with its
  layers in order; the focus panel is a sheet. No connecting lines.
- **Findings page.** A reading-width list grouped by run, each finding a card with its id in mono,
  title, About chips, status pill (open in the attention tone, planned and folded quiet, dismissed
  muted), text in the reading face, and the three actions as quiet buttons. The reason field and
  the entry picker appear in place.
- **Spine rows.** Coverage and Findings rows in the existing row style; the open count as the
  row's count.
- **Help.** A help topic for each page (what the columns mean, the keys, the triage actions) in
  `web/src/plan/help/`, following the existing topics, and the keys in the shortcuts list.
- DESIGN.md: a "Map pages" entry under `## Page rhythm` and the finding status tones under
  `## Status`.

## Proof

As V1: build, start the server against the scratch fixture repository (DEMO plus three findings
from two runs, one dismissed), shoot both pages at 1440x900 and 390x844 in Ledger and Walnut with
a card selected and with the Uncovered filter, check against this brief, fix, re-shoot, report the
paths, restore `web/dist`.
