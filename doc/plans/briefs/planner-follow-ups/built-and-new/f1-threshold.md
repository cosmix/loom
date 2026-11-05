# F1: `/new` as the Threshold, with the follow-up kind (R12)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:

1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `common.md`, this brief, `web/src/plan/ui/DESIGN.md`, the "Home and kickoff" and "Chat
   home and the chat panel" sections of `doc/loom/knowledge/architecture/planner-web-shell.md`,
   and "Planner Polish Rules" in `conventions/plan-web.md`.
3. Open `doc/plans/mockups/new/alt-a-threshold.html` (the specification; its four states) and
   `doc/plans/mockups/chat/alt-d-hearth.html` (the chat the page becomes after Send). Read their
   CSS for values; build with the planner's tokens and components, never by pasting it.

## Settled decisions (do not reopen)

- The mockup is the specification except: after Send the doodle is removed entirely (no shrunken
  corner doodle, no hover animation in that state); one roundel per pane (at most one
  `BusyRoundel` drawn in the conversation column at a time, on what is working now).
- Taking shape keeps its side column in the live kickoff (the mockup's state 4 omits it; the
  frozen "the Name row renames the work" contract needs it).
- The kind switch stays three native radios named exactly `Feature`, `Standalone plan` and
  `Follow-up`, each described by its hint through `aria-describedby`. The frozen contract "Add
  plan opens New work with Standalone plan chosen" queries `radio` by `/Standalone plan/` and
  `/Feature/`, so no other radio's name may contain `Feature` or `Standalone plan`.
- The composer keeps its label `What do you want to build?` (sr-only is fine) and Start keeps the
  accessible name `Start the conversation` (`Try again` after an error); the journey script and
  the tests click `/^Start( the conversation)?$/`.
- A follow-up is a feature chat, not a kickoff: Start sends Plan follow-ups for the picked
  feature (S2's `useKickoffStart`), and the route then opens `/f/<feature>/metaplan`, where the
  chat panel shows the session beside the metaplan it writes.

## Built for you (S2, previous wave; read them, do not edit them)

- `new/kickoff-start.ts`: `WorkKind`, `PickedDocument`, `useKickoffStart(onFollowUp?)` with
  `start(kind, text, documents, feature)`.
- `new/follow-up-features.ts`: `followUpFeatures(checklists)`, `FollowUpFeature`
  (`name`, `built`, `done`, `total`, `status`, `candidates`), `PLAN_FOLLOW_UPS_ID`.
- `feature/built-model.ts`: `followUpSeed(candidate)`, `reviewFile(path)`.
- The feature list is `featuresAtom` (`state/atoms.ts`), the checklists the home page draws.

## Build

1. **Split `new/empty-kickoff.tsx`** (376 lines; nothing may cross 400). Delete its
   `beginKickoff`, `useKickoffStart` and `PickedDocument` and import S2's. Move the attachment
   rows (`AttachmentGlyph`, `attachmentDetail`, `KickoffAttachment`, `KickoffAttachments`) to
   `new/kickoff-attachments.tsx` unchanged in behaviour (frozen: the remove button's name stays
   `Remove`). Starters and follow-up suggestions go to `new/kickoff-starters.tsx`; the picker to
   `new/feature-picker.tsx`.
2. **Draft state.** `useKickoffDraft(initial)` holds `kind: WorkKind` (from the route: `plan` for
   `small`, `follow-up` for a `followUp` feature, else `feature`), `feature: string | null`, the
   text and documents. `EmptyKickoff({ small, followUp, onFollowUp })` takes
   `followUp?: string | null` (a feature to preselect: honoured only when `followUpFeatures` lists
   it) and `onFollowUp?: (feature: string) => void` (passed to `useKickoffStart`).
3. **Kind switch** (`new/kind-control.tsx`): `KindControl({ kind, onChange, disabled })` over
   three choices: `Feature` ("A vision, product sections, a metaplan and plans"), `Standalone
   plan` ("One plan in doc/plans/"), `Follow-up` ("New plans for a feature that is already
   built"). Keep the native radios and `aria-describedby` hints. The visible hint is the chosen
   kind's, beside the switch (mockup `.kind-hint`); the others stay in the DOM for their radios'
   descriptions (sr-only). `shape-size.tsx` imports `kind-control.css` for Taking shape's Size
   control: Size must look as it does now, or move its rules to `taking-shape.css` unchanged.
4. **Picker** (`new/feature-picker.tsx`, a working listbox; the mockup only draws it).
   Choosing Follow-up with no feature picked opens it floating under the switch, and so do the
   "New plans for a feature that is already built" starter and the picked chip's `Change`.
   - `role="listbox"`, `aria-label="Which feature?"`, a visible heading "Which feature is this
     for?", one `role="option"` per `followUpFeatures` entry with `aria-selected`, named by the
     slug and its status (`atlas, done · 2 plans`), drawn as: a 14px mark (a check in
     `--tone-completed` for a built feature; else a progress ring in `--primary` filled
     `done / total`), the slug in mono, the status in `--muted-foreground` with its first word in
     `--foreground`.
   - Opening moves focus to the listbox; the first option is active
     (`aria-activedescendant`). ArrowDown and ArrowUp move (no wrap), Home and End jump, Enter or
     Space picks, a click picks, Escape and a click outside close it and return focus to the
     control that opened it. Picking closes it.
   - The foot reads "↑↓ choose · ↵ pick" and "A done feature offers its open findings".
   - No feature with a metaplan: the listbox holds one line "No feature has a metaplan yet." and
     nothing can be picked.
5. **The picked feature** replaces the hint as a chip: slug in mono, its status, and a `Change`
   button. The placeholder becomes `What should change in <feature>?`, plus
   ` Pick a finding below, or say it in your own words.` when it has candidates.
6. **Start rules.** Blocked reasons, in order: authoring unavailable (as now); starting (as now);
   a follow-up with no feature: `Pick a feature first`; no text and no document: as now. Start
   calls `start(draft.kind, draft.text, draft.documents, draft.feature)`.
7. **Under the field.** For Feature and Standalone plan: the starters as pills, labelled
   "Or begin with one of these", in this order: "A new feature for this project" (feature, text
   as now), "A small fix or improvement" (plan, as now), "Grow standalone plans into a feature"
   (feature, as now), "New plans for a feature that is already built" (sets Follow-up and opens
   the picker; text unchanged), "Start from a document I have" (the document starter). For
   Follow-up with a picked feature: its candidates as pills under the label "Open findings from
   <feature>'s reviews. Each one you pick goes into the field.", each a toggle button
   (`aria-pressed`) named by the candidate's text; pressing it appends `followUpSeed(candidate)`
   on its own line to the text, pressing it again removes that line if it is still there. A
   picked feature with none: one quiet line "No open findings in <feature>'s run reviews.".
   Then the note "Nothing runs until you send." (replaces "Nothing is written until you say
   so.").
8. **Route** (`new/kickoff-route.tsx`): pass `followUp` from `location.state?.followUp` (a string,
   else null) and `onFollowUp = (feature) => { void navigate(featureHref(feature, "metaplan"));
   revealChat(store); }` (`chat/sheet-state.ts`) to `EmptyKickoff`.
9. **Live kickoff head** (`new/kickoff-head.tsx`): the Hearth chat head (mockup state 4
   `.chat-head`): the title is the work's name, else "New feature" or "New standalone plan" by
   kind, else "New work"; under it one muted line with the live state. The roundel
   (`ActivityGlyph`/`RoundelGlyph`) shows there only while the session is `starting`, when no
   turn exists yet; in every later state the transcript's working turn or ending card carries
   the pane's one roundel, so the head draws none. Use the Hearth panel head's state words
   (find them with `rg -n 'Needs your answer' web/src/plan/chat`); idle shows no state word.
   Keep the title an `h1`.

## Fixed visual targets (Walnut and Ledger, 1440px and 390px)

1. **State 1, empty.** One centred column (44rem max; the page grid's third column and
   `.kickoff-side--art` are deleted, with their `display: none` rule). From the top: the
   `LoomDoodle` (about 300px wide, 240px at 390px) drawing its thread once on arrival (the
   existing draw keyframes in `ui/doodle.css`, behind a class or prop only `/new` sets, so home's
   hover-only drawing is unchanged; no animation under `prefers-reduced-motion`), the eyebrow
   "New work" in the accent ink, "What do you want to build?" in the reading face at about 38px
   (30px at 390px), the lead in 15px muted at 56ch; then the field; then the pills; then the
   note in the reading face, italic, muted. Centred text at 1440px, left-aligned at 390px.
   Elements rise in once, staggered about 60ms.
2. **The field.** One rounded field (24px corners, 20px at 390px) on `--card` with a 1px
   `--border` edge and the focus ring on `:focus-within`. Inside: the switch (a 28px pill track,
   chosen segment `--muted` and 600) and its hint on the top line (at 390px the switch fills the
   width, three equal segments, hint under it); the borderless textarea at 16px; the attachment
   rows; the foot with a quiet "Attach a document" (an icon button at 390px), the blocked reason
   in `--type-fine` muted, and Start as a 32px round primary with an up-arrow icon.
3. **State 2, follow-up.** The picker floats under the switch: 380px wide (the field's width at
   390px), `--card`, `--corner-surface`, the popover shadow and 1px ring, the one floating
   surface on the page. After a pick, the chip sits where the hint was, the starters give way to
   the candidate pills, a pressed pill shows a check in `--tone-completed` on `--muted`.
4. **State 3, typed with a document.** The chat composer's chip row inside the field under the
   words; Start lit; the reason line gone.
5. **State 4, after Send** (`/new?session=<id>`): no doodle and no "What do you want to build?";
   the head per Build 9 with a hairline under it; the Hearth transcript from the chat-hearth
   stage (the user's words as the first bubble with its attachment; assistant prose in the
   reading face); the composer at the foot. One roundel in the conversation column at most.
6. **The glow.** A faint radial wash of `--primary` behind the head and field (mockup
   `.page::before`) in both themes; it must not reach the field's text contrast.
7. 390px never scrolls sideways; the picker stays inside the viewport.

## DESIGN.md (you are its only editor this stage)

Rewrite the kickoff exceptions (`kind-control.css`'s two-segment line and the kickoff screen's
type line) to the new truth, and add a "New work" exception naming what departs from the rest of
the planner: the rounded field, the pill starters and suggestion toggles, the round primary
Start, the radial glow, the reading-face heading and note, the entrance motion. F2 (the Built
page) sends you no lines; the main agent adds any it reports.

## Tests (write them; the main agent runs the gate)

`new/kind-control.test.tsx` (three radios, hints, `onChange` with the kind),
`new/feature-picker.test.tsx` (keyboard: arrows, Home, End, Enter, Escape returns focus; click
picks; empty list), `new/empty-kickoff.test.tsx` (update the cases this changes: the hook's
import, the note text, the starters; add: the follow-up starter opens the picker; a picked
feature's candidate pill appends and removes its seed; `followUp` preselects; a follow-up
without a feature shows `Pick a feature first`), `new/kickoff-route.test.tsx` (update the head
assertions; add: a follow-up start navigates to `/f/<feature>/metaplan`; a `starting` session
draws exactly one `.aurora-roundel` in the conversation section; an idle one draws none). Keep
every assertion this plan does not change and name each one you change.

## Contracts (written before you start; make them pass, never edit them)

`web/src/plan/new/threshold.contracts.test.tsx`: "a follow-up starts only after a feature is
picked"; "the feature picker works from the keyboard". Read them first.

## Shots

You cannot build or shoot. The main agent shoots the four states and compares them with the
mockup; misses come back to a fresh fable worker. List in your report the exact steps that reach
each state.

## Files

Your row in the stage's worker table. Never edit S1's, S2's, S3's or F2's files. Before finishing,
`rg` every class name and text you changed or removed across `web/src` and `web/scripts`, and
report each hit outside your files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/new src/plan/home src/plan/ui`, from `web/`.
