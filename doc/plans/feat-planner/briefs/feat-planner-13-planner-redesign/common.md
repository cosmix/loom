# Common brief: planner redesign

Every stage of `PLAN-feat-planner-13-planner-redesign.md` reads this file first.

## Why this plan exists

The operator used the planner after plan 12 closed and found it unusable:

- The feature screen is a status page about the pipeline. Vision and Metaplan show a path that is no link, Pressure test, Review and Run are blank, Conversation lands on Run, and a finished feature opens on the blank Run page.
- Concept mode, the product's differentiator, works but only a header tab or a chip reaches it.
- Buttons are large and full of text, the same offer appears up to five times, text is crude and badly laid out, and the chat's enclosed parts look jarring.
- The chat knows nothing about what the user is reading.

Plan 12 passed every gate because its criteria checked structure, CSS values and copy, and none walked a user task (`doc/loom/knowledge/mistakes/planner-ui-repair.md`, "The Repair Passed Every Gate While the Feature Screen Stayed Unusable"). This plan is accepted by what a user can do. Its driver is the journey script in `journeys.md`.

## Decisions the operator settled

1. **A feature is named by its slug everywhere in the app:** switcher, eyebrow, cards, breadcrumbs, chat and the browser tab. `ChecklistView.title` is removed. A document's own heading stays on that document.
2. **Authoring and pressure sessions run in auto mode.** Summary sessions stay in `default`, where an unlisted tool is denied at once.
3. **The seven-step rail and the step pages go.** The sidebar is a step spine: rows in workflow order that open the real content and expand into documents.
4. **A finished feature opens on the product overview.**
5. **Step 6, Review, is removed from the model.** It records nothing and has no state: it is the user reading, and reading is what the screen is now for.
6. **Chat stays docked on the right, knows what the user is viewing, and is visually redesigned.**
7. **Every visual worker invokes `frontend-design:frontend-design` before writing code.** This is the operator's instruction.
8. **All backend implementation runs on `gpt-6.1-sol` at effort `high`, through `loom-codex-forwarder`.** Fixes and verification stay on Claude. This is the operator's instruction. All Rust implementation is in `session-and-wire` (`session-and-wire.md`, Lanes and units); the later stages own no Rust file, and a backend fix in integration-verify goes to Claude.

## Decisions settled for this plan

### The screen

```text
Logo  feat-planner ▾                     Idle   Home   Files   Settings
───────────────────────┬───────────────────────────┬───────────────────
✓ Vision               │                           │  Chat
✓ Product spec  12/12  │   Reading column          │
    Overview           │   concept mode by default │
    01 Workflow model  │   Source · Edit · Discuss │
    02 Command, server │                           │
    …                  │                           │
✓ Metaplan             │                           │
✓ Plans         12/12  │                           │
    Overview           │                           │
    01 documents  done │                           │
    …                  │                           │
– Pressure test        │                           │
✓ Run        12 done   │                           │
```

- **Header:** one row. Logo, identity (eyebrow plus switcher showing the slug), activity pill, Home, Files, presence, Share, theme, Settings, Help. No Product or Engineering tab, no step rail and no next-step card.
- **Spine:** a `nav aria-label="Feature documents"` on every `/f/:feature` route. It is the only place a feature's progress is drawn.
- **Next step:** one sentence and at most one button at the head of the spine. A feature with no gap and nothing to start shows the sentence only.

### Routes

`/f/:feature` becomes a layout route (`FeatureLayout`, `web/src/plan/feature/feature-layout.tsx`) rendering the spine beside an `Outlet`:

| Route | Renders | Spine row |
| --- | --- | --- |
| `/f/:feature` (index) | redirect by the table below | none |
| `/f/:feature/vision` | the vision's source through `ReadingColumn` (`files/reading-column.tsx:281`) | Vision |
| `/f/:feature/product` | product overview (`ConceptRoute`), with pace and section queue above it while sections are missing | Product spec, Overview |
| `/f/:feature/product/:doc` | section summary (`ConceptRoute`) | a section |
| `/f/:feature/metaplan[/:file]` | a metaplan's source through `ReadingColumn` | Metaplan |
| `/f/:feature/engineering` | engineering overview (`ConceptRoute`), with pace and plan queue above it while plans are missing | Plans, Overview |
| `/f/:feature/engineering/:doc` | plan summary (`ConceptRoute`) | a plan |
| `/f/:feature/pressure` | one row per plan (`pages-and-reading`) | Pressure test |
| `/f/:feature/run` | run state and the completed-feature offers (`pages-and-reading`) | Run |

The vision and metaplans have no summaries by design (`doc/product/feat-planner/index.md`, Summary levels), so they read as source. Everything with a summary opens in concept mode, with Source on the same page. `plans/:doc`, `/new` and `/files/*` keep their routes. `?step=` is dropped with no redirect (the project is unreleased).

**Landing** (`landingPath(checklist)`, `web/src/plan/feature/landing.ts`):

| State | Route |
| --- | --- |
| `has_product` false and no metaplan | `vision` |
| Vision exists, a listed section is missing | `vision` |
| Vision exists, every listed section written | `product` |
| No vision, a metaplan exists | `engineering` |

### Offers

**No screen draws one offer twice.**

- `PrimaryOffer` leaves the chat panel entirely: `state-line.tsx:181` and `new-chat-offers.tsx`. The panel offers New chat and context prompts only.
- On `product` and `engineering` while pace is showing, the page owns the primary offer and the spine shows the sentence only.
- A queue row whose offer equals the primary shows the text "Next" with no button.
- A finished feature has no primary. `RefineSpec` and `NewMetaplanRound` become secondaries, drawn only on the Run page.
- Labels are short, and the sentence beside the button explains. The table is in `session-and-wire.md`.

### Theme and test rules that still bind

`conventions/web-dashboard-typography.md`, `conventions/plan-web.md` (all of it) and these:

- Inter sets chrome and titles, Source Serif 4 (or the chosen reading face) sets reading body, Commit Mono Cosmix sets code, paths and ids.
- `--doc-1..8` hues stay; `concept/doc-tokens.test.ts` pins them. All four themes (Ledger, Aubergine, Pacific, Graphite) must read well.
- 390px must not scroll sideways.
- Server text goes through `<Untrusted>`.
- Web contract tests are top-level `it()` calls whose names equal the contract string.
- Tests over 400 lines split non-assertion setup into helpers.
- A brief ordering pinned-copy tests rewritten raises test-integrity events: keep every guard that still holds, list each lost guard, and file one `dispute-integrity` after the commit naming each file.

## Rules for every worker

- Build what the brief says. Where it is silent on a visual value, the design worker's `web/src/plan/ui/DESIGN.md` decides, and the frontend-design skill decides where that is silent too.
- Run only the one narrow check your brief names. The main agent owns the gate and all git operations.
- Record mistakes, decisions and surprises with `loom memory`. Never use Claude Code auto-memory.
- Knowledge is reference data. When it contradicts the tree, the tree wins and you record `stale-knowledge:`.
