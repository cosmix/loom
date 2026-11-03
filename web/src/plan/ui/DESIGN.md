# Planner visual system

Rules for every planner surface. The scales and primitives are in `design.css` (imported once from `plan.css`); the tone comes from the reading column on the product page: type and space do the work, and the chrome stays out of the way of the text. A rule that fails in one of the four themes (Ledger, Aubergine, Pacific, Graphite) fails here.

## Colour

- Theme tokens only: `--background`, `--card`, `--foreground`, `--muted`, `--muted-foreground`, `--border`, `--hairline`, `--primary`, `--primary-foreground`, `--ring`, `--tone-*`, `--doc-1..8`. No colour literal, no `color-mix` that invents a hue, no gradient.
- `--primary` is the one accent. It appears as the primary button's fill, the current row's mark in the spine, the focus ring (`--ring`) and the arrival flash in the reading column. It is never a border, a tint behind a surface, a heading colour or a status.
- `--tone-*` colours a glyph, never a text run, a border or a background. Warnings use `--tone-warning`; `--destructive` is for an action that deletes.

## Type

Chrome is Inter (`--font-sans`); reading prose keeps `--font-reading`; paths, ids and code keep `--font-mono`. Sentence case everywhere; the `eyebrow` utility is the only uppercase.

| Token          | Size                            | Use                                                              |
| -------------- | ------------------------------- | ---------------------------------------------------------------- |
| `--type-page`  | 20px/650, `.plan-page-title`    | the one title of a screen, under its eyebrow                     |
| `--type-title` | 15px/600, `.plan-section-title` | a section inside a page, a panel head                            |
| `--type-body`  | 14px/400                        | chrome prose: a sentence beside a button, a note, an empty state |
| `--type-ui`    | 13px/500                        | buttons, rows, tabs, labels, the spine                           |
| `--type-fine`  | 12px/400, `.plan-fine`          | a reason under a control, a count, a timestamp                   |

Hierarchy is made with size, weight and space, in that order. A surface uses the weights the scale above names (400, 500, 600, and 650 for the page title) and no others. Muted text (`--muted-foreground`) marks what is secondary; it never marks what is disabled.

## Space

`--space-1..7` = 4, 8, 12, 16, 24, 32, 48px. Inside a control and between a glyph and its word: 1-2. Between rows: 2-3. Between a title and its content: 3. Between sections: 5-6, with no rule and no box between them. Page gutter: `--page-gutter` (24px, 16px below 480px). Reading text keeps its 68ch column; chrome prose stops at 48ch.

## Surfaces and borders

- The page ground is `--background`. One level of surface exists: `.plan-surface` (`--card`, hairline edge, 10px corners) standing on the ground. Nothing inside a surface draws a border, a shadow or another surface. A box in a box is not allowed.
- A border is allowed around: an input, a surface standing alone on the page, a popover or dialog. A border is not allowed around: a button row, a list that sits on the page, a status, a heading, a notice.
- Lists divide with hairlines (`.plan-list`, `.plan-rule`); their edges are the page's edges. A hairline separates; it never encloses.
- Shadows exist only on things that float: popovers and dialogs (`--plan-shadow`, `--dialog-shadow`). Nothing in the page flow casts one.
- An open block in the reading column (a sentence opened to its source passages) draws a 2px edge down its start in its document's hue (`--doc-n`, through `--run-colour`). It is the one coloured border on the page: the hue says which document, never a status.
- The chat composer is a bordered input inside the chat panel, so the place to type stands apart from the transcript around it.

## Buttons

`DisabledReason` draws every button (`.plan-button`, 28px high, 13px/500, 6px corners, one line, `white-space: nowrap`). `OfferButton` wraps it for an offer. Three looks through `look`:

- `quiet`: no edge, muted text until hovered. Toolbar actions, Source and Edit, icon buttons.
- `secondary`: hairline edge, transparent. Every offer and action by default.
- `primary`: `--primary` fill. At most one per screen, and only its owner passes `look="primary"`: the surface that draws `checklist.primary` (the spine's next step, or the pace card holding it on `product` and `engineering` while pace shows), a dialog's confirm (a dialog floats and owns its own), and the Start of the home and kickoff screens. `DisabledReason` is secondary unless told otherwise.

`size="sm"` is the 24px/12px row variant for an action inside a list row, a panel head or the document heading's action row, where every control (Summary, Source, Edit, Discuss, Open all) is a bare `.plan-button` of that size on one 30px line band with the title. A label is a verb plus at most two words (`Start`, `Move`, `Rebuild`), except that an offer's server label is drawn as sent (`Write plan feat-planner-01-documents`); a label carrying a document's slug truncates and never wraps. The sentence beside the button explains, so the button never carries the explanation. A screen never draws one offer twice: each offer has one owner, decided by its id.

Blocked: `aria-disabled`, 50% opacity, no hover change, and the reason beneath in `.plan-fine`, or after the button on its line in the document header's summary status line (`reasonPlacement="inline"`). The reason is the control's accessible description, so it is drawn once per blocked control and never repeated in a second line or a banner under the same control.

## Status

Status is a glyph and a word (`.plan-status`), inline with the text it describes: `✓ done`, `○ not written`, `· optional`, and the current item marked with `--primary`. The word keeps the text's colour; only the glyph takes a tone. No coloured chips, no pill backgrounds, no segmented progress bars, no coloured left borders for a status (an open block's edge, under Surfaces, names a document). A count reads as text (`12/12`, `3 plans`) in `.plan-fine`. An out-of-date item is dashed, not recoloured.

## Page rhythm

Eyebrow, page title, `--space-5`, then content. Sections follow at `--space-6` with a `.plan-section-title`. Rows sit on hairlines at 36px minimum height. The chrome never competes with the reading column: the spine and the chat panel are set in `--type-ui` and `--muted-foreground`, with the current item in `--foreground`. At 390px nothing scrolls sideways. Chrome stays at `--type-ui` or larger, except `--type-fine` text and the mono ids, numbers and paths set at that size.

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
