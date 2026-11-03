# Planner issue fixes: shared context

Every worker reads this file first, then its own brief. The plan is `doc/plans/PLAN-planner-issue-fixes.md` (`doc/plans/IN_PROGRESS-PLAN-planner-issue-fixes.md` once `loom run` has started); its YAML is authoritative for gates and file ownership.

## The operator's issue list (verbatim)

1. Sometimes level 1 summaries expand to almost identical text. This should never happen.
2. arrows/chevrons of expanded block are too subtle. as is the 'border' on the left. it should be more prominent.
3. on the chat: 'end chat' vs 'new chat'. What's the point?
4. 'new chat' output weird.: loom plan: no step chosen (twice printed).
5. chat: additional input context printed on the screen in markup. shouldn't be printed at all.
6. running plans: wrong state, e.g. '0 of 6 plans merged' (when all plans were merged)
7. closed/contracted chat pane -> why does it show the activity roundel? This is poor.
8. 'open all' button -> shouldn't it change to 'fold all' once clicked?
9. summary view: ugly document list at the top of a summary. ugly rebuild all button (misaligned) under 'summary current'.
10. totally misaligned: summary/source/edit/discuss/open all -> generally beautify (frontend-design)
11. Loom logo too small at the top/header. Should be identically sized as the webui.
12. animated play icon ugly. use the rotating arrows to indicate activity.
13. notification pop-up with activity very ugly.
14. duplicated listing of 'plan running' activities in the notification pop-up.
15. changed since you looked dialog very very crude and ugly. explain the rationale. also: doesn't seem to update its content after you've seen it.
16. rebuilding summaries took 2 mins to start! that's a major issue.
17. input area in chat very difficult to distinguish from the rest of the window background.
18. two level summaries seem to be gone.

## Operator decisions (settled; do not reopen)

- **#3:** a live chat's ⋯ menu offers **End chat** only. **New chat** appears only when no chat is live (the head button) or after a chat concluded (the concluded composer area). Ending stays a deliberate act; starting fresh is End chat, then New chat.
- **#15:** keep "Changed since you looked", fix it and redesign it. A file's baseline advances to what you saw when you view its diff. The dialog states in one line what it compares.
- **#1:** the overview model default goes back to **sonnet** (`DEFAULT_PLAN_OVERVIEW_MODEL`), and a seal check rejects an overview sentence that restates a sentence it cites.
- Models: Claude only. Visual work goes to opus at effort medium, using frontend-design (see "Visual work" below). Haiku does small specified fixes, sonnet most development, and opus hard bugs and verification.

## Design rules

- `web/src/plan/ui/DESIGN.md` is the design record. Read it before touching layout, type or colour.
  - Theme tokens only, as DESIGN.md lists them: `--background`, `--card`, `--foreground`, `--muted`, `--muted-foreground`, `--border`, `--hairline`, `--primary`, `--ring`, `--tone-*`, `--doc-1..8`, plus the `--space-*`, `--type-*` and `--corner-*` scales.
  - No colour literal, no gradient, no invented `color-mix`. `--accent` and `--input` are not planner tokens: do not use them.
- `--tone-*` colours a glyph, never a text run, border or background. `--primary` is the screen's one accent, never a border or a status.
- One surface level: a box inside a box is not allowed. An input, a popover and a dialog may have a border.
- Status is a glyph and a word, inline. Do not draw a badge, chip, pill or progress bar. Write the counts instead.
- **Deliberate exceptions this plan makes to DESIGN.md:**
  - (#2) An open block in the reading column carries a 2px edge in its document's hue (`--doc-n`). The hue is identity, not status.
  - (#17) The chat composer is a bordered input inside the chat panel.
  - V1 adds both exceptions to DESIGN.md's Surfaces section, so later workers do not undo them.
- 390px wide must not scroll sideways. Every surface must read in light themes (Ledger) and dark themes (Aubergine).

## Visual work: who, how, and the refinement loop (operator's instruction)

- **Who.** Every visual issue goes to an opus subagent at effort medium (`loom-senior-software-engineer`, spawned with `effort: medium`):
  - V1: #2, #8, #9, #10;
  - V2: #7, #11, #12, #13, #14, #17;
  - O1: #15, behaviour and look.
  A fix round on visual work goes to a fresh opus/medium visual worker, never to haiku or sonnet.
- **How.** A visual worker's first action is to invoke the `frontend-design:frontend-design` skill through the Skill tool, before it reads code. Its brief gives starting values; operator-fixed requirements are marked **fixed**.
- **Refinement loop.** Subagents cannot build the bundle or the binary (`loom-hooks/subagent-verify-guard.sh`; the server embeds `web/dist`), so they cannot see their own work.
  1. After the visual workers return, the stage's main agent builds and shoots the affected screens in Ledger and Aubergine at 1440px and 390px.
  2. If any issue misses its target or the Design direction below, the main agent spawns ONE fresh opus/medium visual worker. Its brief: the shot paths, the issue numbers and the miss list. It also invokes frontend-design first, and it reads the PNGs with the Read tool.
  3. The main agent shoots again. A second miss on the same item gets one more round. A third miss is recorded with `loom memory note` and reported to integration-verify.

## Design direction (frontend-design, set for this plan)

Visual workers invoke the `frontend-design:frontend-design` skill first, as the operator asked, and execute this direction. The skill's general advice on fonts, gradients, textures and bold palettes does not apply here: the planner has a design record, and DESIGN.md wins.

- **Tone: editorial precision.** The planner is a reading instrument, like a well-set technical journal: serif reading prose (`--font-reading`), quiet Inter chrome, mono for paths and ids. It earns its character from exact alignment, a strict type scale and generous, consistent space. It never earns it from colour or ornament.
- **The one thing to get right on every surface is the line.** Every control in a row shares one height and one centre line. Every row's text starts on one column. Every gap comes from the `--space-*` scale, and one gap value is used per relationship (glyph to word, control to control, row to row). The operator's complaints (#9, #10, #13, #15) are almost all broken alignment and stacked wrappers. Fix the grid first; restyle second.
- **Density:** chrome in `--type-ui` and `--type-fine`. One line per row where the content allows. Truncate with an ellipsis rather than wrapping a control row.
- **Motion:** only what explains state. The rotating arrows mean "running"; a fold or unfold may ease its height in 120ms. Everything stops under `prefers-reduced-motion`. No entrance animations and no hover theatrics.
- **Proof:** a worker judging a surface compares Ledger and Aubergine shots at 1440px and 390px. Lay a straight edge mentally down the text column and across the control row: anything off it is a defect.

## Code and test rules (from `doc/loom/knowledge/conventions/plan-web.md`)

- Server and model strings render through `<Untrusted>`. Storage keys go through `storageKey`.
- jest-dom is not installed: assert with Testing Library queries and plain matchers.
- A route-level test goes through `renderAt` (`web/src/plan/concept/concept-route-support.tsx`). Chat frames are seeded through the `FakeSocket` seam in `planSocketAtom`. Stub `matchMedia` for wide layouts.
- A test reads CSS through an `@/...css?raw` import.
- Contract test files are frozen and hold only `it()` calls named by the contract. Never edit a `*.contracts.test.tsx` file or a Rust `*contract_tests.rs` file.
- File limit 400 lines, function limit 50 lines. Split before crossing either.
- When a test pins old behaviour this plan changes on purpose, update that test in the same change, keep its guard assertions that still hold, and name the test in your report. The main agent files one `dispute-integrity` for those after the commit.
- Each stage's gate runs the whole vitest suite (web) or the whole cargo suite (backend). A change to a shared component breaks tests outside your directory: `rg` for the component's importers and for any class, icon or text you change before you finish, and report every hit outside your files to the main agent.
- The browser journeys (`web/scripts/plan-journeys.mjs`, `plan-journeys-kit.mjs`, `plan-journeys-chat.mjs`) are in no stage's files, so these stay as they are:
  - Source, Summary and Edit in the document heading are links (role link, exact names);
  - `button.concept-run-cap`, `.concept-passage` and `.concept-mark[data-dashed]`;
  - a popover is a `PopoverContent` (`data-slot="popover-content"`);
  - the chat panel's `aside[aria-label="Chat"]` textarea, Send, "Loom planner home", "Start a conversation" and the textbox named /What do you want to build/.
- These accessible names are used by frozen contracts and existing tests; keep them even when the visible text changes: the changed-since trigger "Changed since you looked · N", the list "Changed documents", each row's button `Show diff for <path>`, the activity dialog and list "In progress", the details summary "Chat menu".

## Worker rules

- Subagents write code and tests and report. They run no git command and no `bun run build`, `cargo build` or `tsc` (the verify guard blocks them). They run only the focused test command their brief names, once, at the end.
- Record a mistake, a decision or a surprise with `loom memory note "..."` or `loom memory decision "..."` as it happens. Never use Claude Code auto-memory.
- No mention of Claude, Claude Code or any AI system in code, comments or docs.
- Report: files changed, tests run with their result, every pinned test you changed and why, and anything you could not settle.
