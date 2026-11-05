# V3: a redesigned chat sidebar (issues 15, 4, 5, 8 layout, 10 chat half)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:
1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `common.md` (Design rules, D7-D11), `web/src/plan/ui/DESIGN.md`, the "Chat home and the chat panel" section of `doc/loom/knowledge/architecture/planner-web-shell.md`, and the "Chat Panel and Review UI Rules" section of `conventions/plan-web.md`.

## The complaint

The operator calls the whole sidebar unstyled. The clearest case is a composer after two Add to chat selections. Inside one bordered box, three rows stack, each aligned differently:
- "Now viewing section_03 ×";
- two chips, each a book-icon tile, a quote cut mid-word after a lone opening quote mark, a ×, and "section_03 summary" on a second line;
- the placeholder below them.

The Send button floats under the box, away from the paperclip. Stop sits on its own row. The run line "user authoring · 0 watching · 52.2k tokens" wraps. The ellipsis menu holds a bordered button in a bordered box.

Redesign the sidebar as one coherent instrument. You set its look; C1 changes behaviour in parallel (no collapse, menu close keys) and owns `panel.tsx`, `panel-head.tsx`, `prefs.ts` and `offer-flow.ts`. You style the head through CSS, and report any head markup you need to the main agent.

## Fixed targets

1. **References in the composer (issue 15).**
   - The context chip and the reference chips read as one set of attachments: one row anatomy, one alignment, wrapping as a group above the field.
   - A selection chip shows its quote in balanced quote marks on one line, cut with an ellipsis, with its source as `--type-fine` secondary text on the same line or a tidy second line. The label logic stays `chat/chip-label.ts` (`QUOTE_CHARS`); change the presentation only.
   - A file chip shows its name and size; an image chip shows a 32px thumbnail.
   - Remove controls are the same size and hit area everywhere.
   - At 390px nothing scrolls sideways, and the chips never push the field out of view (the attachment area caps its height and scrolls inside the box).
   - Check two, five and ten chips, and a long file name.
2. **Composer (issue 4, D8).**
   - Send is the composer's primary control, on one centre line with the paperclip.
   - In `composer.tsx` and `stop-control.tsx`, Stop sits in `.chat-composer-actions` immediately before Send, at Send's size. `StopControl` keeps its button name "Stop" and its error text.
   - The frozen "composer has a visible border" rule holds.
   - The textarea stays a direct child of `.chat-composer-box`.
   - While a file is dragged over, the box shows "Drop to attach" with no layout shift.
3. **Head and run line (issue 4).**
   - Title, state ("Waiting for you" as a glyph and a word) and menu sit on one centre line.
   - `run-header.tsx`: the run line never wraps. It truncates with an ellipsis, each separator belongs to the segment after it, and under about 22rem the token count drops first.
   - Keep `.chat-run-header` (pressure tests and the help pressure scene query it) and the watchers' `aria-label`.
4. **Menu (issue 5, D9).** `.chat-menu-pop` takes the header popovers' surface (`header/header.css` `.plan-overflow`).
   - In `end-chat.tsx`, End chat is a full-width menu row (`DisabledReason` with `look="quiet"`, left-aligned, its own hover).
   - Keep the `<details>`, `aria-label="Chat menu"` and the name "End chat".
5. **Transcript.**
   - User turns, assistant prose, tool fold line, notices ("Stopped" is one ruled break; issue 3's fix leaves one), permission card, thinking and errors share one rhythm and one gutter.
   - The kickoff transcript on `/new` reads the same.
   - Keep the frozen fold label and DOM (`chat.contracts`).
   - Keep the `.chat-notice-item[data-kind="api_error"] .chat-notice-glyph { color: var(--tone-blocked);` text exactly: `transcript.test.tsx` matches it.
6. **Panel frame (issue 8, D7).** With the rail gone, the panel's edge, resize handle and head read as a fixed column of the page. `chat.css` holds no `box-shadow`, `opacity` or `--tone-` (`panel-a11y.test.tsx`).
7. **Type (issue 10, D11).** In `chat/chat.css`, the hard-coded 13px `.chat-panel` root becomes `var(--type-body)`. In `chat-transcript.css`, `chat-surface.css`, `chat-composer.css` and `chat-decision.css`, every size goes one step up. Assistant prose is 16px.
   - Report the final sizes as `exception:` lines or a "Type" note for V1, who owns DESIGN.md.
8. **Busy states (issue 9, D10).**
   - `chat/state-line.tsx`, `chat/chat-state-glyph.tsx` and `chat/tool-row.tsx` use the D10 roundel for every busy state.
   - `chat/chips.tsx`'s "Uploading N files…" and "Checking attached passages…" use V1's `BusyText` (`web/src/plan/ui/busy-text.tsx`). If it does not exist yet when you reach this step, report it, and the main agent wires it after the wave.
9. **Help scene.** `help/chat-scene.tsx` pins Stop as `.chat-composer-input + div button`. Change it to Stop's new place, and keep every other pin target matching exactly one element (`help/chat-scene.test.tsx`).

## Shots

You cannot build or shoot. The main agent shoots after your pass, at 1440px, 1280px and 390px in Ledger and Graphite, with a Playwright driver for:
- an open chat with two and with five reference chips;
- a running turn;
- the open menu;
- the kickoff on `/new`.

Misses go to a fresh fable worker (common.md loop). Your report lists every state needed to see each surface.

## Files

Your row in the plan's worker table. Never edit C1's, V1's, V2's, B1's or D1's files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/help/chat-scene.test.tsx src/plan/help/pressure-scene.test.tsx src/plan/new`, from `web/`.
