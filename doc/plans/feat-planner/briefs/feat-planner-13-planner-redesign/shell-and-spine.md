# Stage shell-and-spine

Read `common.md` first. Paths are under `web/src/plan/` unless they start with `web/` or `scripts/`. Every worker invokes `frontend-design:frontend-design` before writing code.

## D1: the visual system (fable, explicit model override at spawn)

You set the look the rest of the plan follows. No mockup exists and none is wanted: the operator asked for a designer's judgement. They called the current app "ugly", "crude and rough": big buttons with tons of text, largely unstyled and badly laid out text, nested boxes.

Look at the real app first: run `scripts/plan-screens.sh` per `journeys.md` over `/`, `/f/feat-planner/product`, `/new`. The reading column on the product page is the best part of the app today. Extend its calm to everything else.

**Deliver:**

- `web/src/plan/ui/design.css`, imported once from `plan.css`: a spacing scale, a type scale for chrome, surface and hairline rules, and a button scale (quiet, secondary and one primary) sized for a dense tool. Primary is rare: one per screen at most. Labels are short, and a button never wraps.
- `web/src/plan/ui/DESIGN.md`, at most 120 lines, stating the rules every other worker follows: when a border is allowed (nesting a box in a box is not), how hierarchy is made with type and space, how the single accent is used, how status is shown without coloured chips, the page rhythm, empty states, focus and hover. Include a short "do not" list drawn from what you saw.
- Restyle `ui/disabled-reason.tsx` (`DisabledReason` fixes the button look) and `offers/offer-button.tsx` to the scale. Their callers restyle through wrappers today (`[&_button]:`), so remove the wrappers you find in `header/`, `feature/` and `home/`. Workers in later stages handle the rest.

**Constraints:**

- Use theme tokens only (`web/src/index.css`, `web/src/aurora-ui/shared/styles/tokens.css`); add no colour literal.
- `--doc-1..8` hues stay.
- It must hold in all four themes, in both schemes.
- `chat.css` may not gain `box-shadow`, `opacity` or `--tone-` (`chat/panel-a11y.test.tsx:179-183`).

D1 finishes before the others start.

One check: `node_modules/.bin/vitest run src/plan/ui`.

## S1: header (sonnet)

- Remove `FeatureProgress` (`header/header.tsx:59-72,103`), `step-tracker.tsx`, `rail-popover.tsx`, `next-step.tsx`, `step-rail.css`.
- `readyLabel` and `isCurrentStep` move out or go, with their last consumer in `feature/step-list.tsx`.
- `tabs.ts`: `PLAN_TABS` holds Files only; Home stays beside it. Update `header/tabs.tsx`, `tabs.test.ts:7-30`, `header/tabs.test.tsx`, `router.test.tsx`.
- Help targets `.plan-next-step` (`help/topics.ts:59`, `help/topics.test.tsx:183-191`) and `help/getting-around-scene.tsx:69-74` mount the header. Rewrite those scenes and notes to the spine. `settings/planner-swatch.test.tsx:41` checks the dashed next-step box; point it at a surface that remains.
- Restyle `header.css` to `DESIGN.md`: one row, about 56-64px at desktop, and a compact row at 390px with no sideways scroll.

## S2: spine, layout and landing (sonnet)

Delete `feature/feature-route.tsx`, `step-list.tsx`, `step-items.tsx`, `steps.ts`'s step titles and `feature.css`'s step rules. Keep `plural`, moved to `ui/plural.ts`, with its importers updated (`home/feature-card.tsx:9`, `home/standalone-shelf.tsx:9`, `new/proposal.tsx:8`, `feature/queue.tsx:9`, `feature/pace.tsx:4`, `feature/draft-team-panel.tsx:12`). Note `chat/pressure/steps.ts` is a different file.

**`feature/feature-layout.tsx`** exports `FeatureLayout`: `useParams` feature, `UnknownFeature` when absent (`feature-route.tsx:26-38` moves here), `setActiveFeature`, then `<Spine />` and `<Outlet />`. Below 760px the spine collapses behind a "Documents" control and the current row names itself; it never scrolls sideways.

**`feature/spine.tsx`** exports `Spine`:

- A `nav aria-label="Feature documents"`.
- At its head: the next-step block from `feature/next-action.tsx` (`NextAction`).
- Rows from `checklist.steps` and the document lists:

| Row | Target | Children |
| --- | --- | --- |
| Vision | `vision` | none |
| Product spec | `product` | Overview, then `checklist.sections`: written rows link to `product/section_NN`, missing rows are plain dim text with "not written" |
| Metaplan | `metaplan` | `checklist.metaplans` when more than one |
| Plans | `engineering` | Overview, then `checklist.plan_rows` linking to `engineering/<slug>` with state |
| Pressure test | `pressure` | none |
| Run | `run` | none |

- The current route's group is expanded; the rest collapse and remember per-feature through `storageKey`. Selected row: `aria-current="page"`.
- Row marks come from `step.mark` (`done`, `gap`, `active`, `optional`), with counts from `step.count`.
- A feature with no product section hides nothing: Product spec reads "not started".
- Section names are Markdown phrasing: render through `SentenceText` as `queue.tsx` does. All other server strings go through `<Untrusted>`.

**`feature/next-action.tsx`:** one sentence plus one `OfferButton` for `checklist.primary`, per the Offers rules in `common.md`. On `product` and `engineering` while `paceOffers` returns choices, render the sentence only. Reuse the copy logic from the deleted `next-step.tsx`, using `offer.feature` (the slug).

**`feature/landing.ts`:** `landingPath(checklist)` per `common.md`. The index route is `<Navigate replace>` to it.

**`feature/document-page.tsx`:** `VisionPage` and `MetaplanPage` render `ReadingColumn path anchor={null} tree` (`files/reading-column.tsx:281`), calling `useFileSocket(store)` as `file-mode.tsx:97` does. A missing vision shows `EmptyOffer`-style copy with the `StartVision` offer. Nothing else on that page may draw that offer.

**`router.tsx`:** nest the feature routes per `common.md`, keeping `plans/:doc`, `new` and `PLAN_TAB_ROUTES`. `ConceptRoute` keeps reading `:feature`, `:doc` and the pathname; `pageKeyOfPath` (`api/page-key.ts`) keys on path segments, so confirm that `/f/x/product` and `/f/x/engineering` still map. `home/home.tsx:89-101` still redirects a one-feature repository to `/f/<feature>`.

**Queue and pace.** `feature/queue.tsx` and `feature/pace.tsx` render above the overview on `product` (section queue) and `engineering` (plan queue) while `paceOffers(checklist, step)` has choices. Mount point: a `feature/gaps.tsx` that `ConceptRoute` renders through a slot prop, or that `FeatureLayout` renders above `Outlet` for those two routes; choose the one that keeps `ConceptRoute`'s scroll container intact and record it with `loom memory decision`.

Queue row links go to the concept routes (`product/section_NN`, `engineering/<slug>`), replacing `toAddress` at `queue.tsx:147`. `queue.tsx`'s `isQueuedPlan` and text parsing go: rows come from `checklist.plan_rows`. A row equal to the primary offer shows "Next" and no button. `draft-team-panel.tsx` stays on the same two pages.

**Tests.**

- `feature/feature-route.test.tsx`, `step-list.test.tsx`, `header/step-tracker.test.tsx`, `header/next-step.test.tsx` and `header/header.test.tsx:131-133,230` go or change with what they pinned; list every lost guard for the main agent.
- `home/features-contracts-kit.tsx:13` imports `FeatureRoute`: update it.
- `concept/concept-route-support.tsx` `renderAt` must render through the nested routes.

Add `feature/spine.test.tsx`, `landing.test.ts`, `next-action.test.tsx`.

## S3: the journey script (sonnet)

Build `scripts/plan-journeys.sh` and `web/scripts/plan-journeys.mjs` per `journeys.md`. Run it against this stage's routes before reporting; journeys that need `pages-and-reading` or `chat-redesign` are written now and marked `pending` in the script's output until those stages land, by checking whether the route or element exists at all.

## Main agent

After the workers:

1. Run `node_modules/.bin/oxfmt`.
2. Build, then run the journey script and `scripts/plan-screens.sh --worktree-doc` over `/f/feat-planner/product`, `/f/feat-planner/vision`, `/f/feat-planner/metaplan`, `/f/optim-token/engineering` in all four themes.
3. Read every shot against `DESIGN.md` and fix deviations through a fresh worker.
4. Run one `loom check shell-and-spine` beside the final `loom-code-reviewer` round, whose brief carries `DESIGN.md` and the screenshot paths.
