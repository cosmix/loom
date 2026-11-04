# V3 — the New work screen, the kickoff conversation and Taking shape (issues 2, 9, 10)

Tier: opus, effort medium (`loom-senior-software-engineer`). Spawned after K2 returns. First invoke `frontend-design:frontend-design` with the Skill tool. Then read `common.md` (Design rules, Visual work), `web/src/plan/ui/DESIGN.md`, and this brief.

## The operator's complaint

- Typing the first message changes the whole screen.
- The kickoff header "New work · kickoff conversation" (`new/kickoff-head.tsx:25`) sits above an opening item that also reads "New work".
- The type is small.
- The Taking shape panel looks unfinished.
- It is unclear whether you are starting a plan or a feature.

## Fixed targets

- **One screen, two states.** The empty form and the live conversation share one layout: one column, the same width, the same heading position. Starting a conversation does not move the heading or swap the page's structure.
  - The prompt the user typed becomes the first user turn, where the textarea was.
  - Taking shape appears beside the conversation at 1024px and up, and below it under 1024px.
  - Each state is one `KickoffRoute` render. Restyle K2's markup; do not fork it.
- **Heading.** One heading per state. The empty form: "What do you want to build?" (journeys need the textbox name). The live kickoff: the draft's name, else "Untitled", with the kind of work as a quiet word. The kickoff header no longer repeats "New work" above the opening item (`new/kickoff-head.tsx`). This heading text is the one behaviour change you make; update the heading-text assertions in K2's tests (`kickoff-route.test.tsx`, `taking-shape.test.tsx`) for it and name each in your report.
- **Kind control names.** Keep the two radios named "Feature" and "Standalone plan" (stage contract `add-plan-preselects-standalone`), and the Taking shape element's `aria-label="Taking shape"`.
- **Kind of work** (K2's control): reads as the first decision on the page, a two-option segmented control with a one-line description under each option ("A vision, product sections, a metaplan and plans", "One plan in doc/plans/").
- **Unfinished notice** (K2): one quiet line above the form with its two actions. It never reads as an error.
- **Draft view** (`/new?session=<id>` of an ended draft): a short document with the name, then problem, decisions, questions and "Your messages", with Continue as the screen's one primary action.
- **Taking shape** (`new/taking-shape.tsx`, `taking-shape.css`, `proposal.css`):
  - rows are label-then-value on one column;
  - lists are prose bullets, not boxes;
  - "Not decided yet" style empties read as muted words;
  - the Rename control sits at the value's end;
  - one surface level, no nested boxes.
- **Type:** the kickoff transcript follows the chat stage's type floor (`common.md`). `new/kickoff.css:281-290` keeps the reading face for assistant prose.
- **Home's Unfinished shelf** (K2's `home/drafts-shelf.tsx`): rows aligned on one text column, the name, then the problem's first line in `--muted-foreground`, then the state word. It reads as a list of work in progress, above the feature cards.

## DESIGN.md

You own `web/src/plan/ui/DESIGN.md` in this stage. Add your exceptions, and the chat stage's exceptions that the main agent passes you (the `exception:` lines in V2's report, recorded in the chat stage's memory).

## Run once at the end

`node_modules/.bin/vitest run src/plan/new src/plan/home src/plan/help` from `web/`.
