# Planner Feature Screens

> Layout route, spine, offers

A feature's screens sit under one layout route. Product contract: `doc/product/feat-planner/` (index, `section_05.md`, `section_06.md`); the design record is `web/src/plan/ui/DESIGN.md`, set by a designer worker and read by every later worker.

## Layout route and landing

`/f/:feature` is `FeatureLayout` (`feature/feature-layout.tsx`): the spine beside an `Outlet`, with no spine while `projectAtom` is null. Children: `vision` and `metaplan[/:file]` (source through `ReadingColumn`, because those documents have no summaries), `product[/:doc]` and `engineering[/:doc]` (`ConceptRoute`, concept mode with Source on the same page), `pressure` and `run` (`pressure-page.tsx`, `run-page.tsx`). `plans/:doc`, `/new` and `/files/*` keep their routes; `?step=` is gone with no redirect. `landingPath(checklist)` (`landing.ts`) sends the index route to `vision` while the vision or a listed section is missing, to `product` once every listed section is written, and to `engineering` for a feature with only a metaplan. A path no child matches renders a `LoadState` error.

## Spine

`Spine` (`spine.tsx`, model in `spine-model.ts`: `spineGroups`, `groupOfPathname`) is `nav aria-label="Feature documents"`, the only place a feature's progress is drawn. Groups are the six steps in workflow order (`GroupId`: vision, product, metaplan, plans, pressure, run); the route's own group is always open, the others toggle and persist per feature through `projectPref("spine-expanded")`. `NextAction` (`next-action.tsx`) heads it: one sentence and at most one button; a feature with no gap shows the sentence only. The optional pressure mark is applied in the spine alone, so the server step mark, the home card's sr-only list and `loom plan check` still report Pressure test done where the spine shows it optional ([follow-ups](../concerns/planner-ui-repair-followups.md#planner-redesign-follow-ups)). The pace choice, draft-team panel and section or plan queue (`gaps.tsx`) mount above the overview through `ConceptRoute`'s `above` slot, on `product` and `engineering` only.

## Offers have one owner each, decided by id

No screen draws an offer twice. `paceOfferIds(checklist, scope)` and `pageDrawsPrimary(checklist, pathname)` (`page-offer.ts`) decide whether the page draws the checklist's primary; when it does, the spine shows the sentence only. Owners: the pace cards on an overview (only when they draw that primary's id), a missing document's page, a written plan's page (its Pressure test button for its own slug and its Start a run), a Pressure row, and the Run page for every `start_run` primary. A queue row whose offer equals the primary reads "Next" with no button. A finished feature has no primary; `RefineSpec` and `NewMetaplanRound` are secondaries drawn on the Run page only. `PrimaryOffer` is not in the chat panel, which offers New chat and context prompts.

## Pressure and Run pages

Pressure: one row per plan (`checklist.plan_rows`) with its last pressure date or "Not tested"; the lede counts live plans with a date, never `checklist.untested`, which counts written plans only ([the mistake](../mistakes/planner-redesign.md#page-text-was-accepted-without-reading-it-against-live-state)). Run: `useRunRead` fetches `/api/run/state` once on arrival, and a failed read says so instead of "Nothing is running"; the server sends no `run_state` frames, so a page that shows run state names what refreshes it. `fetchRunState` (`run/use-run.ts`) applies an overtaken answer while `runStateAtom` is null, because the debug server answers one request at a time (about 5 s for `state_view`). Start a run targets the `start_run` offer's plan, else the preselected run-state plan, else the first written row, only for a written row of this feature.

## Wire and names

The wire has six steps (`planning/checklist.rs::Step`: Vision, ProductSpec, Metaplan, Plans, PressureTest, Run; `Step::name()` is the one display name). The `Review` step is gone, and so is `ChecklistView.title`: a feature is named by its slug in the switcher, eyebrow, cards, breadcrumbs, chat and the browser tab, and a document keeps its own heading. `wire/checklist_card.rs::card_facts` fills `metaplans` and `plan_rows` on the view (rows come from the cached `Feature.metaplans` entries, winning plan paths, `Project::text` and `latest_pressure_date`; the conversion lives in `checklist_rows.rs`). `plan_rows` are listed from the metaplan's plan headings, not a table. `wire/fixtures/*.json` have no writer test (`checklist_fixture_matches_serde_output` only reads them), so they are hand-edited when `ChecklistView` changes, and a removed `Step` variant is searched for in `loom/tests/fixtures/planning/check_schema.json`.
