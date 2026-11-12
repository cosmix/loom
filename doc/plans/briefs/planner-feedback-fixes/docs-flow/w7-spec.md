# W7: product spec text (codex terra, high)

Read `common.md` beside this file first, then the plan's Decisions D1 to D9. Every decision the
plan makes must be stated in the spec as current truth. The waves before you implemented D1 to D5.
The other stages implement D6 to D9 in parallel, and their decisions are final.

## Files you own

`doc/product/feat-planner/index.md`, `section_01.md`, `section_03.md`, `section_04.md`,
`section_05.md`, `section_06.md`, `section_08.md`, `section_09.md`, `section_10.md` and
`section_11.md`.

## What to change (find each with `rg -n` in the files)

- D2, engineering overview. Known hits: `index.md` lines 53, 151 and 311 to 315; `section_01.md`
  312 and 326; `section_03.md` 53; `section_04.md` 26, 34, 66 to 72, 158, 179 and 370;
  `section_05.md` 5, 17, 25, 32, 53 to 54, 61, 68, 217, 230 and 279; `section_08.md` 417 to 447;
  `section_09.md` 494; `section_11.md` 27 and 380. There is no engineering overview: the metaplan
  page carries the Run panel, the gaps, the pace and the queue, and the landing goes there.
- D1: plans and draft-all wait for a settled metaplan, with the disabled reason text. Put this where
  the spec describes writing plans.
- D3, D4: the spine heads go amber while any document is unsettled; a running plan's row pulses a
  green dot; the roundel marks only the header activity pill.
- D5: Settle all specs and Settle all plans. `section_10.md` about line 176 describes Mark settled;
  add the batch beside it, including the confirm for documents with open points.
- D6: the agent ends a finished chat itself, after writing decisions and open questions into the
  documents, with no closing write. A chat the user ends runs the closing write. Find the chat-ending
  text with `rg -n -i 'closing write|end chat'` (`index.md`, `section_08.md`, `section_09.md` and
  `section_11.md` hold it).
- D7: the turn's changed files fold into one summary; tool use is folded by default.
- D8: the diff view opens on rendered markdown, scrolled to the first change, with a Source toggle.
- D9: popups and dialogs have a visible raised edge in every theme.

## Rules

- State current behaviour only: no "now", "no longer", "was" or change history.
- Keep each file's heading structure. Edit sentences in place; do not append a changelog.
- Every fenced code block names a language. `web/node_modules/.bin/markdownlint-cli2` must pass on
  every file you touch.

## Done

The markdownlint command passes on your files, and `rg -n -i 'engineering overview'
doc/product/feat-planner` prints nothing. Report every section you changed, one line each.
