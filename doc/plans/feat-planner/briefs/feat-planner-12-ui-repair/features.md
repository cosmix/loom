# Stage features

Read `common.md` first. Working directory `web/`; paths are relative to `web/src/plan/`.

Target, from the mockup's Features tab notes:
- "The invitation card leads. Hover it and the thread draws itself again, from a spool, through
  a thought, into cloth on the loom. Features follow; each is a pair of folders,
  `doc/product/<name>/` and `doc/plans/<name>/`."
- "The top level of `doc/plans/` holds only standalone plans… They sit on one shelf."
- "All new work starts with a short conversation… Nothing is written until you press 'Write the
  vision' (feature) or 'Write the plan' (standalone)."
- "After the vision, choose the pace."

Defects the browser tour found:
- no "Features" heading;
- the invitation card is a full-width white box with the doodle above the text;
- cards show slugs, empty space or the raw "Status:" paragraph in about 22px serif;
- progress shows as seven "!"/check pills;
- the standalone shelf is about 50 always-open rows that push the page past 1500px;
- `/new` has no description, a bare textarea and plain-text starters;
- `/f/<feature>` is a centred checklist dump.

## F1: home, cards, shelf, doodle (sonnet)

**Files owned:**
- `home/**`
- `ui/doodle.tsx`
- `ui/doodle.css`
- `ui/doodle.test.tsx`

**Page** (`home/home.tsx:100-109`):
- Padding `24px clamp(16px,3vw,32px) 30px`, grid, gap 18px.
- Head: the eyebrow "doc/ in this repository" over an `h2` "Features" (22px/650, letter-spacing -0.02em).
- One grid `repeat(auto-fill, minmax(min(100%, 290px), 1fr))`, gap 14px, holding the invitation
  card first (spanning two columns from 620px up, one below) then the feature cards. Keep a
  `ul aria-label="Features"` for the cards by making the invitation the first `li` or by
  rendering the cards' `li`s inside the shared grid.
- Then the shelf.
- Keep the DOM order invitation, cards, shelf (`home.test.tsx:198-215`).

**Invitation card** (`home.tsx:53-77`):
- A `<section>` named "Start something new" (keep the region and heading names), made
  clickable as a whole through a stretched `<Link to="/new">` on the CTA.
- Grid `minmax(0,1.15fr) minmax(0,1fr)`, gap `8px 18px`, padding `18px 20px`.
- Border `1px solid color-mix(in oklch, var(--primary) 55%, var(--border))`, radius 14px.
- Background `radial-gradient(120% 140% at 0% 0%, color-mix(in oklch, var(--primary) 14%, var(--card)) 0%, var(--card) 60%)`.
- Hover and focus-within: `box-shadow: 0 0 0 1px var(--primary), var(--plan-shadow)` and
  `translateY(-1px)`, 160ms.
- The doodle sits in the left column. The right column holds:
  - the heading at 18px/650, letter-spacing -0.02em (keep the heading role; level `h2`);
  - a paragraph in the reading face at 16px/1.5, `--muted-foreground`: "Bring a rough idea. Loom
    talks it through with you, then shapes it into a feature with its own spec, or a single
    plan.";
  - the CTA: 34px high, padding `0 14px`, radius 9px, `--primary` fill, 13.5px/600,
    `MessageCircle` icon, "Start a conversation";
  - the sub-line: 12px muted, "Nothing is written until you say so."
- Keep the blocked-authoring `DisabledReason` path and the empty variant ("No features yet.").

**Doodle** (`ui/doodle.tsx`, `ui/doodle.css`), viewBox `0 0 270 128`:
- Ink (stroke `--muted-foreground`, width 1.6, round caps and joins):
  - a spool at the left: an ellipse stack about 26 wide, x 18-44, y 70-104;
  - a loom frame at the right: a rectangle x 150-238, y 52-112 with six vertical warp lines.
- Weft: one path with `pathLength="1"`, stroke `--ring`, width 2.1. It leaves the spool, loops
  once through a "thought" curl around (100, 40), enters the frame and weaves across the warps
  in three passes, ending at (204, 92).
- End dot: a circle r 2.6 at (204, 92), fill `--ring`.
- Three sparks: small plus marks in `--ring`, width 1.6, near the curl.
- Motion, on hover or focus of `[data-doodle-host]`:
  - weft `stroke-dasharray: 1`, animating `stroke-dashoffset` 1 to 0 over 2.4s with
    `cubic-bezier(.45,.05,.35,1)`;
  - sparks `twinkle` (scale 1.5 and rotate 45deg at 50%, opacity 0.6) over 1.2s, three times;
  - end dot `pop` (opacity 0 until 90%, then 1) over 2.4s.
- At rest the drawing is complete. Keep the reduced-motion block, the `aria-label` and `<title>`
  (`doodle.test.tsx`).

**Feature card** (`home/feature-card.tsx:10-45`), still one whole-card `<Link>` with `aria-labelledby`:
- Grid, gap 12px, padding 16px, 1px `--hairline` border, radius 12px, `--card` fill.
- Top row: flex, gap 10px.
  - A new `home/feature-mark.tsx`: a 30px DocMark-style square (radius 8px). It is tinted by
    `--doc-<n>`, where n = 1 + (stable hash of the slug mod 8), and holds the `Layers` glyph at
    16px.
  - Beside it, the title `h3` (15.5px/600) from `checklist.title`, over a mono 12px muted path
    line:
    - "product/<slug>/ · plans/<slug>/";
    - "plans/<slug>/ · no product spec" when `has_product` is false;
    - "product/<slug>/ · no plans yet" when the feature has no plans directory (`plans.total`
      is 0 and no metaplan step is done).
- Pitch: reading face, 15.5px/1.5, `--muted-foreground`, inline code at 0.8em, clamped to three
  lines. Render it through `SentenceText(visibleControls(...))` as today. When the pitch is
  null, show nothing.
- Progress: replace `ProgressStrip` with a 7-column grid, gap 3px, cells 5px high, radius 3px:
  - done step `--tone-completed`;
  - the current step `--primary`;
  - a running step 7 `--tone-executing`;
  - any other `--muted`.

  Keep an sr-only list naming each step and its mark.
- Facts: flex, gap 14px, 12px, `--muted-foreground`, each an icon plus text:
  - the running activity glyph when `plans.running > 0`;
  - `BookOpen` "<sections.length> sections";
  - `ListChecks` "<plans.total> plans";
  - `Gauge` "<untested> untested" when > 0;
  - `CircleCheckBig` "all done" when every step is done.

  Drop "N of 7 steps done" and "N gaps" from view; fold them into the sr-only text.
- Rewrite `progress-strip.test.tsx` and `feature-card.test.tsx` for this; keep the `Untrusted`
  assertions for slug and pitch.

**Standalone shelf** (`home/standalone-shelf.tsx:80-126`):
- A `<details>`, collapsed by default, inside a `section` that keeps the region name "Standalone
  plans". 1px `--hairline` border, radius 12px, `--card` fill.
- Summary row: flex, wrap, gap 10px, padding `12px 14px`, 13.5px/600:
  - a `Wrench` icon;
  - "<n> standalone plans";
  - a muted "· bugfixes, performance work and small features in doc/plans/";
  - then, at `margin-left: auto`, the two existing actions ("New standalone plan" with `Plus`,
    "Move into a feature…" with `FolderInput`) as ghost buttons. Their clicks must not toggle
    the details (`preventDefault` plus `stopPropagation`).
- Body: flex-wrap chips, gap 6px, padding `0 14px 14px`.
  - Each chip is a link to the plan: 26px high, padding `0 8px`, 1px `--hairline`, radius 6px,
    mono 12px, holding the slug and a state dot in its tone.
  - Show the first 24, then a "+N more" button that reveals the rest.
- Keep the `MoveRows` offers for misplaced visions and metaplans above the chips.
- Keep the region and button names the tests use (`home.test.tsx:207,245,273`,
  `standalone-shelf.test.tsx:67`).

Proof: `node_modules/.bin/vitest run src/plan/home src/plan/ui/doodle.test.tsx`, once.

## F2: the feature screen (sonnet)

**Files owned:**
- `feature/**`

Do not edit `header/**`. The shell stage writes its own narrow rail list.

**Layout** (`feature/feature-route.tsx:43-74`):
- Grid `220px minmax(0,1fr)`, `min-height: 480px`. Below 760px it becomes one column, with
  the navigation as a horizontal scroller.
- `?step=<n>` (`useSearchParams`) selects the step shown; the default is `current.number`, or 7
  when ready.

**Navigation:**
- `nav aria-label="Steps"`, `--card` fill, 1px `--hairline` right border, padding `18px 14px`,
  grid gap 4px.
- One link per row: "Conversation" (done), then the seven server steps.
- Each row: grid `26px 1fr`, gap 8px, padding 8px, radius 8px, 13px.
  - A 26px round badge with 1px `--border`.
  - Done: fill `--tone-completed` 20%, transparent border, check glyph.
  - Current or selected: `aria-current="step"`, row fill `--muted`, weight 600, badge border
    `--primary` and fill primary 25%.
  - Otherwise: `STEP_GLYPHS[n-1]` in muted.
- Under each label, a 11.5px muted line:

  | Step | Line |
  | --- | --- |
  | Conversation | the slug |
  | Vision | "<sections.length> sections listed" |
  | Product spec | "<written> of <listed> written" |
  | Metaplan | "after the spec", or "written" |
  | Plans | "<plans.total> plans" |
  | Pressure test | "<untested> untested", or "all tested" |
  | Review | "optional" |
  | Run | "<running> running", or "<done> done" |

- Keep `step-list.test.tsx:36-77`'s order. Rewrite the rest of it for links.

**Body:**
- Padding `24px clamp(16px,3vw,32px)`, grid gap 16px.
- An eyebrow "<title> · step n of 7" over an `h1` (20px/650, letter-spacing -0.02em) with the
  step's action title:

  | Step | Title |
  | --- | --- |
  | 1 | Write the vision |
  | 2 | Write the product spec |
  | 3 | Write the metaplan |
  | 4 | Write the plans |
  | 5 | Pressure-test the plans |
  | 6 | Review what was built |
  | 7 | Run the plans |

- Then the selected step's content:
  - the pace choice (steps 2 and 4 while they have gaps);
  - the team panel while a team runs;
  - the section queue (step 2) or plan queue (step 4);
  - the step's checklist items with their offers (from `ChecklistGaps`, filtered to that
    step).

**Pace** (`feature/pace.tsx:57-77`):
- Two cards in `repeat(auto-fit, minmax(min(100%,240px),1fr))`, gap 10px.
- Each card: padding `12px 14px`, 1px `--border`, radius 10px, `--card` fill.
- Title row:
  - `Footprints` "Guided, one section at a time" (plans: "one plan at a time");
  - `Users` "Draft all with a team".
- Copy:
  - Guided: "A session drafts one section with you, stops, and offers the next once you've read
    it. Best when the vision is still moving."
  - Team: "A team of up to five drafts every missing section at once. Teammates agree terms and
    boundaries as they write; the lead checks the set for consistency, then hands you a review
    queue."
  - Plans versions say "plan".
- Each card keeps its own `OfferButton` at the bottom. Keep `aria-label="Choose a pace"`, the
  heading "Draft all, with a team" as an accessible name (visible text may read "Draft all with
  a team"; set the `aria-label`), and the guided label `Start guided writing: …`
  (`pace.test.tsx`).

**Section queue** (step 2; the plan queue at step 4 has the same shape):
- One bordered list, radius 10px, hairline dividers.
- Rows: grid `34px minmax(0,1fr) auto`, gap 10px, padding `9px 12px`, `--card` fill.
  - A 20px DocMark-style badge with the mono number ("01").
  - The section name (13.5px/600), linking to the file or summary.
  - On the right, a state: `CircleCheck` "Written" in ok; `MessageCircleQuestion` "<n> open
    questions" in warn when > 0; the drafting glyph "Drafting…" while a teammate holds it;
    `CircleDashed` "Missing".
- Data: `checklist.sections`; plans from the step-4 items.

**Team panel** (`feature/draft-team-panel.tsx`):
- Teammates become cards in `repeat(auto-fill, minmax(min(100%,180px),1fr))`, gap 8px.
  - Each card: grid `30px minmax(0,1fr) auto`, padding `8px 10px`, 1px `--hairline`, radius
    10px.
  - A 30px round avatar with the initial, tinted by a hash of the name over `--doc-1..8`.
  - The name in bold, with a mono `§NN` from the current document.
  - The state: "drafted" with check, or "writing" with the drafting glyph.
  - Mono compact tokens on the right.
- The summary and Stop go into one cost line: radius 8px, `--muted` fill, 12.5px, a `Coins` icon,
  "<n> teammates · <m> documents · <tokens> tokens so far", and a ghost "Stop the team" button
  on the right.
- Keep the confirm text, the H2 "Drafting with a team", the "Document queue" and "Teammates"
  lists (`draft-team-panel*.test.tsx`).

Proof: `node_modules/.bin/vitest run src/plan/feature`, once.

## F3: the kickoff (sonnet)

**Files owned:**
- `new/**`

**Empty state** (`new/empty-kickoff.tsx:225-273`, `new/kickoff.css`):
- A centred grid, gap 14px, padding `36px 20px 34px`.
- The doodle at max-width 420px.
- `h1` "What do you want to build?" at 24px/650.
- A paragraph in the reading face, 17px, max-width 56ch, `--muted-foreground`: "Describe it the
  way you would to a colleague: the problem, who has it, what you have in mind. Loom asks
  questions, pushes back, and suggests a name and the sections a spec needs."
- One composer card, width `min(640px,100%)`, padding 12px, 1px `--border`, radius 14px, `--card`
  fill, `--plan-shadow`:
  - the textarea in the reading face at 17px, min-height 70px, borderless;
  - an action row (space-between) with "Attach notes or a document" (`Paperclip`, opening the
    existing `DocumentStarter` picker) and a primary "Start" button (`SendHorizontal`).
  - Keep the textbox name "What do you want to build?". Give the button the accessible name
    "Start the conversation" (or "Try again" on retry), so `empty-kickoff.test.tsx` and
    `kickoff-route.test.tsx` regexes hold.
- Starters: centred, wrapping pills, 30px high, padding `0 12px`, 1px `--border`, radius 999px,
  `--card` fill, 12.5px, each with an icon:
  - `Lightbulb` "A new feature for this project";
  - `Wrench` "A small fix or improvement";
  - `FolderInput` "Grow standalone plans into a feature";
  - `FileUp` "Start from a document I have" (opens `DocumentStarter`).

  Behaviour unchanged (`useKickoffDraft.choose`).
- Keep "Nothing is written until you say so." under the starters.

**Conversation** (`new/kickoff-route.tsx:36-62`):
- Grid `minmax(0,1fr) 330px`, min-height 600px.
- Chat head: padding `12px 16px`, 1px `--hairline` bottom border, `MessagesSquare` "New work",
  muted "· kickoff conversation", and the session state pill on the right (reuse the chat
  panel's state component if one exists; read `chat/` before adding one).
- Transcript: max-width 70ch, centred. Assistant prose in the reading face at 16.5px/1.55; user
  messages in the reading face at 16px.
- The taking-shape panel is on the right.

**Proposal** (`new/proposal.tsx`):
- It stays in the panel, above the shape rows.
- Restyle: padding `12px 14px`, border
  `1px solid color-mix(in oklch, var(--primary) 50%, var(--border))`, radius 10px, fill
  primary 9% over card.
- A title row with `Sparkles` "Proposed: a feature" (or "a standalone plan").
- One line: "Name `<name>` and <n> sections: <names joined by commas>." Use only the name part
  when there are no sections.
- Buttons: primary "Use this" (`Check`), ghost "Rename" (`Pencil`), "Change sections"
  (`ListPlus`), "Make it standalone" or "Make it a feature" (`Wrench`).
- Behaviour unchanged (`proposal.test.tsx`).

**Taking shape** (`new/taking-shape.tsx`):
- The aside keeps the name "Taking shape" and its `h2`.
- `--card` fill, padding 18px, gap 14px, a `Telescope` icon in the heading.
- Rows: grid `22px 1fr`, gap 8px, 12.5px. Each has a bold label line and a muted value, with
  icons by state:
  - `CircleCheck` in ok when the field is present;
  - the drafting glyph in `--ring` while being worded;
  - `MessageCircleQuestion` in warn for open questions;
  - `ListOrdered` muted for sections.
- Size row: a segmented "Feature | Standalone plan" whose clicks send the existing proposal
  messages ("Make this a standalone plan instead." and its feature twin). It reads "Not decided
  yet" with neither pressed until the shape has a size.
- Name row: a mono chip (12px, radius 7px, 1px `--hairline`).
- Sections row: mono "01".."NN" numbers with the titles.
- At the bottom:
  - the notice box (radius 8px, `--muted` fill, 12.5px): "Nothing is written yet. 'Write the
    vision' creates `doc/product/<name>/index.md` with the open questions in it." Use the
    standalone wording for a plan, and drop the path while no name is settled.
  - the large primary write button ("Write the vision" with `Feather`, or "Write the plan").
    It is disabled until `target` is set, with the existing `DisabledReason` text.

  Move both out of `kickoff-route.tsx:55-62`.
- Keep the help-scene selector `aside[aria-label="Taking shape"] > section:nth-of-type(1)`
  working (the Size row stays the first `section`), or record the new selector with
  `loom memory note` for the chat stage, which owns `help/**`.

Proof: `node_modules/.bin/vitest run src/plan/new`, once.
