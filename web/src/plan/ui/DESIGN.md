# Planner visual system

Rules for every planner surface. The scales and primitives are in `design.css` (imported once from `plan.css`); the tone comes from the reading column on the product page: type and space do the work, and the chrome stays out of the way of the text. A rule that fails in one of the four themes (Ledger, Aubergine, Pacific, Graphite) fails here.

## Colour

- Theme tokens only: `--background`, `--card`, `--foreground`, `--muted`, `--muted-foreground`, `--border`, `--hairline`, `--primary`, `--primary-foreground`, `--ring`, `--tone-*`, `--doc-1..8`. No colour literal, no `color-mix` that invents a hue, no gradient.
- `--primary` is the one accent. It appears as the primary button's fill, the current row's mark in the spine, the focus ring (`--ring`) and the arrival flash in the reading column. It is never a border, a tint behind a surface, a heading colour or a status.
- Graphite's accent (`tokens.css`, `.dark[data-dark-theme="gray"]`) is `oklch(0.72 0.14 272)` for `--primary` and `--ring`, with `--primary-foreground: oklch(0.18 0.005 270)`: a periwinkle on the greys' own 270 axis, at the highest chroma sRGB holds at that lightness. Measured contrast (WCAG relative luminance): the foreground on the fill 7.4:1, the fill against `--background` 7.4:1, against `--card` 6.8:1 and against `--muted` 5.9:1, so the button, the spine's current mark, the ring and the arrival flash all pass 3:1 on every ground they stand on. Ledger, Aubergine and Pacific keep their accents.
- `--tone-*` colours a glyph, never a text run, a border or a background. Warnings use `--tone-warning`; `--destructive` is for an action that deletes.

## Type

Chrome is Inter (`--font-sans`); reading prose keeps `--font-reading`; paths, ids and code keep `--font-mono`. Sentence case everywhere; the `eyebrow` utility is the only uppercase.

| Token          | Size                            | Use                                                                                                                 |
| -------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `--type-page`  | 20px/650, `.plan-page-title`    | the one title of a screen, under its eyebrow                                                                        |
| `--type-title` | 15px/600, `.plan-section-title` | a section inside a page, a panel head                                                                               |
| `--type-body`  | 14px/400                        | chrome prose: a sentence beside a button, a note, an empty state; the spine's rows and the chat's rows and controls |
| `--type-ui`    | 13px/500                        | buttons, rows, tabs, labels; in the spine and the chat, what is `.plan-fine` elsewhere                              |
| `--type-fine`  | 12px/400, `.plan-fine`          | a reason under a control, a count, a timestamp                                                                      |

The two sidebars sit one step up the scale from the rest of the chrome, because they are read beside the reading column, not glanced at. In the spine (`feature/spine.css`): rows, the next-step sentence and the group names at `--type-body`; the mono ids and numbers, the counts, the state words, the pressure record and the "Source" line at `--type-ui`. In the chat panel (`chat.css` root is `--type-body`): chrome (head state word, run line, tool rows and fold line, notices, chip labels, context row, queue rows, permission band) at `--type-body`; the head title at `--type-title`; user turns and the composer field at 15px (`--type-title`, turns at 500, field at 400); the assistant's prose at 16px/1.55 in `--font-reading`; secondary text (chip source, file size, timestamps, counts, `.plan-fine`, `size="sm"` buttons) and mono tool and permission bodies at `--type-ui`. The kickoff transcript inherits the chat's sizes. Full-size buttons in the chat are 14px. Nothing in either sidebar drops below 13px.

Hierarchy is made with size, weight and space, in that order. A surface uses the weights the scale above names (400, 500, 600, and 650 for the page title) and no others. Muted text (`--muted-foreground`) marks what is secondary; it never marks what is disabled.

## Space

`--space-1..7` = 4, 8, 12, 16, 24, 32, 48px. Inside a control and between a glyph and its word: 1-2. Between rows: 2-3. Between a title and its content: 3. Between sections: 5-6, with no rule and no box between them. Page gutter: `--page-gutter` (24px, 16px below 480px). Reading text keeps its 68ch column; chrome prose stops at 48ch.

## Surfaces and borders

- The page ground is `--background`. One level of surface exists: `.plan-surface` (`--card`, hairline edge, 10px corners) standing on the ground. Nothing inside a surface draws a border, a shadow or another surface. A box in a box is not allowed.
- A border is allowed around: an input, a surface standing alone on the page, a popover or dialog. A border is not allowed around: a button row, a list that sits on the page, a status, a heading, a notice.
- Lists divide with hairlines (`.plan-list`, `.plan-rule`); their edges are the page's edges. A hairline separates; it never encloses.
- Shadows exist only on things that float: popovers and dialogs (`--plan-shadow`, `--dialog-shadow`; the activity popover layers `--popover-lift`, a longer drop, under `--plan-shadow`). Nothing in the page flow casts one.
- An open block in the reading column (a sentence opened to its source passages) draws a 2px edge down its start in its document's hue (`--doc-n`, through `--run-colour`). It is the one coloured border on the page: the hue says which document, never a status.
- The chat composer is a bordered input inside the chat panel, so the place to type stands apart from the transcript around it.
- exception: the document heading's Summary / Source switch (`.doc-heading-views`, `doc-heading.css`) is a hairline-edged track around its two segments, with the current view filled in `--muted` and set in `--foreground` at 600. A two-state control has to show its state at rest in every theme, and a pair of quiet buttons cannot; the track is a row control's height (24px), so it keeps the action row's line band.
- exception: the document key under a summary's heading (`document-key.tsx`, `concept-page.css`) sets each document on a fixed-width grid track (mark, mono number, title cut by `text-overflow: ellipsis`), with the title in `--foreground` among muted chrome. Without hovering, the title is the only thing that says which document a colour is; the track keeps a long key in columns.
- exception: on a feature document's source route the spine's current row carries a `--type-fine` "Source" line under its name, drawn by CSS (`spine.css`, `data-view="source"`), not a DOM node: the heading's switch already names the view to assistive technology.
- exception: the kickoff's two segmented choices, Kind of work on the empty form and Size in Taking shape (`new/kind-control.css`), are a hairline-edged track around their two segments, the chosen one filled in `--muted` and set in `--foreground` at 600, as the document heading's switch is. Kind of work is the first decision on the page and has to read as one choice between two at rest; each segment carries its one-line description under its label, and the whole segment is the hit area.
- exception: on the kickoff screen (`new/kickoff.css`) the user's words, typed into the form and shown as turns, are set at 15px/400 (`--type-title` size at body weight), and the assistant's prose at `--reading-size` on its 68ch measure. The words typed into the form become the conversation's first turn in the same place and the same face, and the kickoff is read as a document, so its type is the reading column's, not the chat panel's.
- exception: inside Taking shape (`new/taking-shape.css`) `.plan-fine` text (a reason under a control, a status) is set at `--type-ui`, and the name and section numbers are mono at `--type-ui`: the panel keeps the 13px floor of panel text.
- exception: a chat chip's source and a file chip's size are set at `--type-ui` (13px, the floor) in muted text, a step under the chip's quote or name, and sit on its one line beside it (`chat-chips.css`).
- exception: the chat menu's row (`chat-surface.css`, `.chat-menu-item .plan-button`) is a quiet button stretched to the pop's width and left-aligned by a wrapper rule: a menu row has no other owner for its width and alignment.
- exception: `.chat-menu-pop` and `.chat-passage` draw a 1px ring as `box-shadow: 0 0 0 1px color-mix(in oklch, var(--foreground) 10%, transparent)`, the header popover's `ring-foreground/10`: a transparency of the foreground, no invented hue.
- exception: inside the chat panel (`chat-surface.css`), Tailwind `text-xs` lifts to `--type-ui` and `text-[13px]` to `--type-body`, so components shared with other screens follow the sidebar's type step without a second class.
- exception: while a file is dragged over the chat composer, the composer box's edge dashes and turns `--foreground` (`chat-composer.css`, `[data-dragging]`), so the box reads as the one drop target; only the edge's style and colour change, so nothing shifts.
- exception: the app bar (`header/header.css`, `.plan-header`) is washed from `--card` at its top to `--background` at its hairline, the dashboard header's `bg-linear-to-b from-card to-background`. It is the one gradient in the planner: the two headers belong to one product and have to read as one.
- exception: the app bar's Home and Files links (`.plan-view-switch`, `header/header.css`) sit in a hairline-edged track the control height tall, the current view filled in `--muted` and set in `--foreground` at 600, as the document heading's switch is and as the dashboard's view switch is. The current view has to read at rest in every theme beside the cluster's quiet buttons.
- exception: a selected row of the standalone plans table (`home/standalone-shelf.css`, `tr[data-selected]`) keeps the `--muted` hover wash while it is selected, so the selection reads across the row and not only in its box.
- exception: the changes page (`review/changed-since.css`, `.changes-page`) spans the main column's full width, with no 68ch or 80rem cap, and from 1024px fills the main column's height with a fixed 22rem list beside the diff, each scrolling on its own and the foot fixed under both. A side-by-side diff needs the width, and the list has to stay in view beside a long diff.
- exception: on the changes page under 1024px (`.changed-since__detail`) the open document's diff is drawn inside the list item under its row, without repeating the path, so the diff reads next to the row that opened it.
- The footer (`.plan-footer`, `ui/design.css`) pairs with the app bar: a hairline on its top edge, the page gutter, `--type-fine` in `--muted-foreground`. Its layout is the dashboard footer's grid (`routes/shell.tsx`): from 1024px a `minmax(0, 1fr)` track holds the provider meters, so they measure a real width and sit side by side, and an `auto` track at the end holds the "limits unavailable" notes (`.plan-footer__notes`); under 1024px the two tracks stack and the meters take their own narrow layouts.

## Buttons

`DisabledReason` draws every button (`.plan-button`, 28px high, 13px/500, 6px corners, one line, `white-space: nowrap`). `OfferButton` wraps it for an offer. Three looks through `look`:

- `quiet`: no edge, muted text until hovered. Toolbar actions, Source and Edit, icon buttons.
- `secondary`: hairline edge, transparent. Every offer and action by default.
- `primary`: `--primary` fill. At most one per screen, and only its owner passes `look="primary"`: the surface that draws `checklist.primary` (the spine's next step, or the pace card holding it on `product` and `engineering` while pace shows), a dialog's confirm (a dialog floats and owns its own), and the Start of the home and kickoff screens. `DisabledReason` is secondary unless told otherwise.

`size="sm"` is the 24px/12px row variant for an action inside a list row, a panel head or the document heading's action row, where every control (Edit, Discuss, Open all, and the Summary / Source switch's track, under Surfaces) is a bare `.plan-button` of that size on one 30px line band with the title. A label is a verb plus at most two words (`Start`, `Move`, `Rebuild`), except that an offer's server label is drawn as sent (`Write plan feat-planner-01-documents`); a label carrying a document's slug truncates and never wraps. The sentence beside the button explains, so the button never carries the explanation. A screen never draws one offer twice: each offer has one owner, decided by its id.

Blocked: `aria-disabled`, 50% opacity, no hover change, and the reason beneath in `.plan-fine`, or after the button on its line in the document header's summary status line (`reasonPlacement="inline"`). The reason is the control's accessible description, so it is drawn once per blocked control and never repeated in a second line or a banner under the same control.

## Status

Status is a glyph and a word (`.plan-status`), inline with the text it describes: `✓ done`, `○ not written`, `· optional`, and the current item marked with `--primary`. The word keeps the text's colour; only the glyph takes a tone. No coloured chips, no pill backgrounds, no segmented progress bars, no coloured left borders for a status (an open block's edge, under Surfaces, names a document). A count reads as text (`12/12`, `3 plans`) in `.plan-fine`. An out-of-date item is dashed, not recoloured. The document key writes `changed` in `.plan-fine` beside a document that changed since the reader looked.

Work in progress is one sign: the roundel (`BusyRoundel`, `@/aurora-ui/feedback/BusyRoundel`), the black-and-white disc that turns while work runs, rests upright and dims when it pauses, and stops turning under `prefers-reduced-motion`. It is always inside an `aria-hidden` wrapper, so the words beside it are the only announcement and the caller keeps its own role. `ActivityGlyph` draws it for every working kind (`running`, `pressure`, `codex`, `drafting`, `summaries`, `starting`), sized to the glyph box it replaces; `permission` and `waiting` keep their bell and LED, because they mean "your turn". `LoadState state="loading"` draws it with its title under `role="status"`, and `BusyText` (`ui/busy-text.tsx`) puts it before a text-only busy message. The disc is monochrome by design, so no tone colours it; the pressure strip keeps its determinate bar, which carries a count.

A plan row's pressure record (`.plan-spine__badge`, `feature/spine.css`) follows the rule: a flask in `--tone-completed` and "Pressure-tested <date>", or a gauge in the row's colour and "Not pressure-tested". It never wraps under the plan's name; the name truncates first, and in a row under 20rem the date leaves view (and stays in the link's name) before anything else gives.

## Page rhythm

Eyebrow, page title, `--space-5`, then content. Sections follow at `--space-6` with a `.plan-section-title`. Rows sit on hairlines at 36px minimum height. The chrome never competes with the reading column: the spine and the chat panel are set in `--muted-foreground`, with the current item in `--foreground`, at the sidebar sizes under Type (rows at `--type-body`, their counts, ids and notes at `--type-ui`; the chat one step above its old sizes, its prose at 16px). The spine keeps its 18rem column at that size, so a plan's slug truncates sooner and its state and pressure record keep their width. At 390px nothing scrolls sideways. Chrome stays at `--type-ui` or larger, except `--type-fine` text and the mono ids, numbers and paths set at that size outside the two sidebars.

## Empty states

One sentence in `.plan-empty` (14px, muted, 48ch) set where the content would start, and at most one secondary button beside or below it. No illustration, no dashed box, no card. The home and kickoff doodles are the exception and stay on those two screens only.

## Focus and hover

- Focus: `outline: 2px solid var(--ring)` at 2px offset on every focusable thing, the same as the reading column. No ring shadows, no inner glows.
- Hover: `--muted` wash on quiet and secondary buttons and on rows that open; the primary darkens through `color-mix` with its own foreground. Transitions are 120ms and off under `prefers-reduced-motion`.
- A link in chrome underlines on hover only; a link in reading prose is always underlined.

## Do not

Drawn from the screens before this system:

- Do not put a bordered button inside a bordered card, or a card inside a card grid's card.
- Do not repeat the banner's text under a blocked button (home's second `loom plan is in read-only mode` line): the reason line is enough.
- Do not make a label a sentence (`Move into a feature…`).
- Do not fill a button with `--primary` unless it is the screen's one primary.
- Do not tint a surface with the accent, or draw a gradient behind a card.
- Do not draw a six-segment coloured progress bar on a card; write the counts.
- Do not set two mono paths in a row under a title; one, truncated, is enough.
- Do not set a banner in two lines of toned text; one line, glyph toned, text in `--foreground`.
- Do not hatch or stripe a panel to show read-only; dim the control and state the reason.
- Do not cast a shadow on an in-page card or restyle a button from a wrapper (`[&_button]:`, `.x button`); pass `look`.
