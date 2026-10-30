# Stage chat-help

Read `common.md` first. Working directory `web/`; paths are relative to `web/src/plan/`. This
stage runs after shell, features and reading have merged. Read their `loom memory` notes
(`loom memory show --all`) for changed selectors before touching help scenes.

## C1: the chat panel (opus, loom-senior-software-engineer)

**Files owned:**
- `chat/**` except `chat/pressure/**`

This is debugging work. Reproduce each defect in a test first.

Defects the browser tour found, with a session waiting on a Bash permission:
1. The permission card renders twice: once inside the transcript scroller (clipped) and again
   below it.
2. The panel's header block overlaps the clipped top of the transcript.
3. The home screen said "No chat" while that session existed and was shown under every feature,
   optim-token included. A session belongs to the feature (or plan) it was started for:
   - show it only on that feature's routes and its documents' file views;
   - elsewhere show the empty state, plus a one-line notice "A session in <feature title>
     needs you" with a link.
4. The next-step offer raised the native "End the current chat first?" confirm when no chat was
   open on the screen, then again on the next navigation, and a second time right after Cancel.
   - The confirm may fire only when a session is actually open on this screen and the offer
     would replace it.
   - Cancel must not re-raise it.
5. The window title read "(!) loom plan" for that permission. Keep the marker, and add the
   feature title: "(!) <feature title> · loom plan".

Restyle to the mockup's chat (Shell tab):
- **Panel:** a three-row grid (`auto 1fr auto`), `--card` fill, 1px `--hairline` left border,
  13px. Default width 340px: `DEFAULT_WIDTH` in `chat/prefs.ts:8-10` becomes 21.25 rem; the
  resize range is unchanged.
- **Head:** padding `12px 16px`, `min-height: 53px`, 1px `--hairline` bottom border. A chat
  icon, then the session title in bold ("Refine section 01"), then on the right a state pill
  (common pill measures) with the state LED (`waiting` breathes in `--tone-completed`) and
  "Waiting for you", "Working", "Needs a decision", and so on. With no session the head reads
  "Chat", with a "New chat" ghost button on the right.
- **Transcript:** padding 16px, gap 14px.
  - User message: right-aligned, max-width 88%, padding `9px 12px`, radius `12px 12px 4px 12px`,
    fill `color-mix(in oklch, var(--primary) 22%, var(--card))`.
  - An attached quote sits inside it first: a block with padding `5px 9px`, radius 6px,
    `--background` fill, in the reading face at 14px, `--muted-foreground`.
  - Assistant turns: grid gap 7px.
  - Tool rows: 12px muted, icon plus a verb ("Read", "Edited") and mono 11.5px target, wrapping
    anywhere.
  - Changed-files chips: 12px, 1px `--hairline`, radius 6px, padding `2px 8px`, a diff icon and
    "section_01.md +6 −1" where counts exist.
  - A "N summaries rebuilding" chip uses the turning rebuild glyph.
  - The permission card appears once, at the end of the transcript.
- **Composer:** 1px `--hairline` top border, padding 12px, grid gap 8px. The box has min-height
  64px, 1px `--border`, radius 10px, padding `8px 10px` and a `--background` fill. The action
  row holds the attach icon button and, at `margin-left: auto`, a primary "Send" (28px).

Keep every accessible name and storage key the chat tests pin
(`conventions/plan-web.md`, "Chat Panel and Review UI Rules").

Proof: `node_modules/.bin/vitest run src/plan/chat --exclude "src/plan/chat/pressure/**"`, once.

## C2: pressure test (sonnet)

**Files owned:**
- `chat/pressure/**`

Mockup note: "A dialog with the plan, rounds and the three models from `[pressure]`, with what
the run changes… The run takes the authoring slot, the header gets a run strip (shell stage
wrote `header/pressure-strip.tsx`), the chat shows each round… The plan's record line is written
and the review is ready."

Defects the browser tour found:
- the plan card shows the lowercased slug "adopt ecc best practices";
- there are no state chips, and "Change" is an underlined link;
- a stray number input sits beside the 1/2/3 control;
- raw labels show ("/pressure model", "Codex effort");
- the round frame is a thin box with a dangling connector.

**Dialog** (`chat/pressure/pressure-dialog.tsx:51-88`, `pressure-dialog-parts.tsx`):
- **Frame:** width `min(660px,100%)`, radius 16px, 1px `--border`, `--card` fill, scrolling
  inside at `max-height: calc(100% - 8px)`.
- **Hero:** grid `auto 1fr auto`, gap 14px, padding `18px 20px 16px`, 1px `--hairline` bottom
  border, fill `radial-gradient(120% 160% at 0% 0%, color-mix(in oklch, var(--primary) 16%, var(--card)) 0%, var(--card) 62%)`.
  - A 44px badge (radius 12px, primary 28%) holds the animated gauge at 24px.
  - "Pressure-test a plan" at 17px/650.
  - "Two reviewers read the plan at once, then an editor addresses both reviews." at 13px muted.
- **Body:** padding `18px 20px 20px`, gap 18px.
- **Plan card:** grid `auto minmax(0,1fr) auto`, gap 12px, padding `12px 14px`, 1px `--hairline`,
  radius 12px, `--background` fill.
  - The large DocMark, then the plan's real title (its first H1, from the plan document or
    `StandalonePlanView.title`, never the slug) at 14.5px/600, the mono file name, and a chip
    row: the state ("Written"), "Changed <ago>", and "Never tested" (`warn`) or "Tested <date>".
  - On the right, an outline "Change" button with `ArrowLeftRight`. Keep the accessible name
    containing "Change".
- **"What happens":** a label row with "What happens" on the left and "Rounds" plus a pill
  group on the right.
  - The pill group: radius 999px, padding 3px, `--background` fill; buttons 30x26, the pressed
    one `--primary`. It offers 1, 2 and 3.
  - Remove the free number input. Update `pressure-dialog-layout.test.tsx:114-166`.
- **Round frame** (`round-diagram.tsx:18-53`):
  - Border 1.5px dashed `color-mix(in oklch, var(--ring) 55%, var(--border))`, radius 14px,
    padding `26px 16px 16px`.
  - Grid `minmax(0,1fr) 46px minmax(0,1fr)` over two rows.
  - A tag pill at `top: -11px; left: 14px`, holding a `Repeat` icon and "Each round, once" or
    "Each round, N times".
  - Review sits at column 1 row 1, Second review at column 1 row 2, and Address at column 3
    spanning both rows, centred.
  - The middle column is an SVG of two bezier paths merging into one arrowhead toward Address,
    stroked `--muted-foreground` at 1.5 and 0.7 opacity.
  - Below 600px: one column, wires hidden.
  - Keep `role=group aria-label="One pressure-test round"`.
- **Node:** padding `10px 12px`, 1px `--border`, radius 12px, `--card` fill.
  - A head row: a 26px icon tile (radius 8px, tinted `--doc-n`) with the glyph at 15px, and
    the title at 13px/600 ("Review", "Second review", "Address").
  - A 12px muted line ("Claude `/pressure` · a team of 3", "Codex `$pressure`", "Claude
    `/address`").
  - A "background" pill on Codex.
  - The `.pick2` row: model and effort selects side by side, 28px high, 1px `--hairline`,
    radius 8px, `--background` fill, mono 11.5px, reading as "opus · high".
  - The select labels become visually hidden; they keep their accessible names.
- Under the frame: a green dot and "Then a record line goes at the end of the plan: date,
  models, rounds."
- **Facts:** a 2-column grid of icon rows (12.5px muted, bold lead), keeping today's copy:
  - `Clock`: the existing time sentence, with no minute figures;
  - `FilePen`: "Edits the plan and its brief bundle";
  - `FilePlus`: "Writes planning-omissions.md";
  - warn icon: "Ends '<chat>' first, with its closing write", only when a chat is open.
- **Footer:** `--background` fill, 1px `--hairline` top border, padding `14px 20px`.
  - On the left, `Lock` "Holds authoring until it finishes".
  - On the right, ghost Cancel and primary "Start pressure test" with `FlaskConical`.

**Run, in the chat** (`chat/pressure/pressure-header.tsx:212-234`):
- **Run head:** grid gap 8px, padding `12px 16px`, 1px `--hairline` bottom border.
  - Top row: the gauge glyph, "Pressure test" at 13.5px bold, a tag "round r of R", and a
    danger-outline "Stop" (26px, `Square` icon) at `margin-left: auto`.
  - Under it, the mono plan file name.
  - Keep `className="chat-run-header"`, `data-session-id`, and the confirm flow.
- **Timeline:** grid gap 2px.
  - An uppercase 10.5px label per round ("ROUND 1").
  - Rows: grid `20px 1fr auto`, 12.5px:
    - a done step shows an ok check;
    - running steps show their animated glyph;
    - queued steps show a muted dashed circle;
    - each row has a right-aligned mono time.
  - Derive the queued rows from `run.rounds` and the step order (Codex and Claude `/pressure`
    in parallel, then `/address`, then "Write the record line").
  - Keep each step group's `aria-label` "Round N <label>".
- **Codex card:** grid `26px 1fr auto`, padding `10px 12px`, 1px `--hairline`, radius 10px,
  `--background` fill.
  - Bold "Codex `$pressure`", then "<model> · reviewing the plan in the background", then a
    mono elapsed.
  - Keep the sr-only `role=status`.

**Done** (`chat/pressure/pressure-done.tsx:59-89`):
- Run head: an ok `CircleCheck`, "Pressure test finished", and a tag "<R> rounds · <duration>".
- Summary box: padding 14px, 1px `--tone-completed` 45% border, radius 10px, fill
  `--tone-completed` 8%. It holds a bold "Both rounds addressed" (or "<n> rounds addressed") with
  one muted line, then the edited files as changed-file chips, then a primary "Review the
  changes" (`ScanEye`) and a ghost "Test the next plan" (`Gauge`).
- Keep `role=region aria-label="Pressure test summary"` and `aria-label="Edited files"`. If
  "Review the changes" stops being the last `div`'s `a`, tell C3 the new selector through
  `loom memory note`.

Proof: `node_modules/.bin/vitest run src/plan/chat/pressure`, once.

## C3: help (sonnet). Starts after C1 and C2 return.

**Files owned:**
- `help/**`

Mockup note: "Help, in the dashboard's form. Each topic is a set of scenes; each scene a
real-component illustration with numbered pins… hovering a note rings its target. Facts without
a control go in 'Good to know'. Legend and Shortcuts are full topics. `?` opens it anywhere."

Defects the browser tour found:
- in "Getting around", pin 3 sits on Share instead of the next-step card, and pin 2 hangs at the
  far left of the rail;
- in "Pressure tests", pin 3 floats outside the card, and scene 2's pin 1 overlaps the title;
- the header scene shows a six-step rail with "Plan review" and a scrollbar.

Every scene renders production components, which the earlier stages changed. Re-target every
selector to the new DOM: the header and rail (shell), the invitation, cards, shelf and taking
shape (features), the reading column and caps (reading), and the run head and done box (C2). The
sample data uses the real seven steps with `count` and `label`, and titles.

Layout:
- **Dialog:** width `min(1040px, calc(100vw - 3rem))`, `min-height: 640px`, max height 90dvh,
  radius 14px. A grid of 260px plus 1fr.
- **Navigation:** padding `16px 12px`, 1px `--hairline` right border, a gradient fill from
  `--background` to `color-mix(in oklch, var(--background) 60%, var(--card))`.
  - Brand: the logo at 18px and "Help" at 14px/650.
  - Search: 34px high, radius 9px, `--card` fill, with the placeholder "Search help" and a
    `Kbd` "/".
  - Topic rows: grid `32px 1fr`, padding `7px 8px`, radius 10px, gap 4px between rows. A 32px
    tile (radius 9px) in the topic hue, with the glyph at 16px. The current row has a solid hue
    tile with the glyph in `--background`, a `--card` fill and an inset hairline.
  - The label is 13px/600; the description is 13px (not 11.5px, per `common.md` decision 8),
    single line with an ellipsis.
- **Main:** padding `26px 30px 24px`, gap 20px.
  - Hero: a 52px badge (radius 14px, glyph 26px), the eyebrow "Topic n of 7", the title at
    22px/650, and an intro in the reading face at 17px muted, max-width 62ch. No bottom rule.
- **Scene card:** padding 14px, 1px `--hairline`, radius 14px, `--card` fill.
  - Head: a pill "Scene n" (22px high, radius 99, `--muted` fill), the title at 14px, and on
    the right "Hover a note to find it" with `MousePointer2`.
  - Canvas: padding 32px, radius 11px, `--background` fill with a 14px dot grid of
    `color-mix(in oklch, var(--muted-foreground) 26%, transparent)`.
  - The component is lifted (radius 10px plus shadow); wide components such as the header
    render at 74% scale with full layout width.
- **Pins:** 22px `--primary` discs, 11.5px/700, `box-shadow: 0 0 0 3px var(--background)` plus a
  drop shadow. The hot pin scales to 1.22 and pulses; the target gets a 2px `--ring` outline at
  offset 4px.
  - **Fix placement** (`help/pin-placement.ts`): a pin anchors to the whole control group (the
    union box of its selectors).
  - It takes the first of above, left, below and right that stays inside the canvas and
    covers no target or placed pin. Measure after the component has laid out (the
    `ResizeObserver` plus timers).
  - Add a test for each defect above.
- **Notes:** `repeat(auto-fit, minmax(min(100%,260px),1fr))`, gap 6px.
  - Each note is a grid `24px 1fr`, padding `8px 10px`, 13.5px/1.45, hover `--muted`.
  - The number disc is 22px at primary 30%.
- **Good to know:** grid `auto 1fr`, padding `14px 16px`, radius 14px, fill primary 9%, border
  primary 30%, a 32px `Lightbulb` tile, and bullets as 6px `--ring` dots.
- **Footer:** grid `1fr auto 1fr`. Previous and next topic cards sit at the sides, with
  "`?` opens this guide · `Esc` closes" in the centre.
- **Legend:** grouped "Reading", "States" and "In progress". Each entry is a 40px glyph tile with
  its name and meaning, and the list is complete.
- **Shortcuts:** "While reading" and "Anywhere" groups, with `Kbd` at least 26px wide and a 2px
  bottom shadow. Rows reflect the real keys, `a` for attach and `c` for the composer.

Reading-scene copy, one scene with four pins:
- sentence: "A sentence that opens. Hover shows its document's colour; click or press Enter."
- mark: "Which document the sentence draws on. A dashed mark is being rebuilt."
- cap: "Fold back to the sentence you opened. Esc folds one level, Shift+Esc all."
- source chip: "The source passage, with its file and heading. It opens in file mode."

Pressure-scene copy, one scene with three pins:
- run head: "The plan, the round and the step. Stop ends the whole run."
- Codex card: "Codex reviews in the background; its report links here when written."
- Claude step: "Claude reviews at the same time, then a second session addresses both reviews."

Update the help tests for the new copy and selectors.

Proof: `node_modules/.bin/vitest run src/plan/help src/plan/shell.test.tsx`, once.
