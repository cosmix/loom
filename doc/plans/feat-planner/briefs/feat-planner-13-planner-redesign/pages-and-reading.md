# Stage pages-and-reading

Read `common.md`, then `web/src/plan/ui/DESIGN.md` (from the merged `shell-and-spine` stage). Paths are under `web/src/plan/`. Every worker invokes `frontend-design:frontend-design` before writing code.

## P1: Pressure test and Run pages (sonnet)

**`feature/pressure-page.tsx`** exports `PressurePage`, at `/f/:feature/pressure`. It has one row per `checklist.plan_rows`, each showing:

- the slug, linking to `engineering/<slug>`;
- the plan's state;
- the last pressure date (`pressure_date`) or "Not tested";
- for an untested writable plan, an `OfferButton` for its step-5 item offer (`checklist.items`, `item.step === 5`, matched on `item.path === row.path`). The primary offer, drawn here, is not drawn again in the spine (`common.md`).
- for a tested plan, a "Record" link to `/files/<path>` through `toAddress`; `files/plan/plan-view.tsx:11-28` shows the `## Pressure tests` section there.

A live run's strip stays in the activity pill and chat. An empty feature gets one sentence naming what comes first, and links to Plans.

**`feature/run-page.tsx`** exports `RunPage`, at `/f/:feature/run`, built from `runStateAtom`, `runStripAtom` (`run/use-run.ts:20`) and `checklist.plan_rows`:

- a line for overall state: nothing running, running with merged/total, or done;
- the start action through the existing `run/plan-run-actions.tsx`;
- a dashboard link when `dashboard_url` is set;
- one row per plan with its state, each linking to its concept page.

Once every plan is done, the page holds the secondaries `RefineSpec` ("Compare spec with build", with one sentence explaining that it starts a chat comparing the done plans and their run reviews with the spec) and `NewMetaplanRound`.

The global run state covers only the active plan, so per-plan progress beyond state is not shown; that is deliberate. Revisit if the operator asks for per-stage progress here.

**Tests:** `feature/pressure-page.test.tsx`, `feature/run-page.test.tsx`, and route entries in `router.tsx`.

One check: `node_modules/.bin/vitest run src/plan/feature`.

## P2: reading surface (sonnet)

Apply `DESIGN.md` to everything a reader sits in:

- `concept/concept.css`, `concept/document-key.tsx`, `concept/page-heading.tsx`, `concept/marks.tsx`, `concept/run.tsx`, `concept/passage.tsx`
- `ui/doc-heading.tsx`
- `markdown/markdown.css`, which is largely unstyled per the operator; headings, lists, tables, quotes and code need real hierarchy
- `files/doc-heading-view.tsx`, `files/plan/*`, `editor/*` chrome only
- `concept/empty-states.tsx`

**Specifics the operator complained about:**

- An opened passage is a heavy highlighted box. It reads as text that grew in place (`doc/product/feat-planner/index.md`, Vision), set in the same type; a quiet edge is enough to mark it and to fold it.
- Mark squares carry no explanation. Give the first sentence on a page a one-time quiet hint, dismissed per browser through `storageKey`, or put the legend in the document key; choose per `DESIGN.md`.
- The document key is chips in a box. Now that the spine lists documents, reduce it to what the spine cannot show: which colour means which document.

**Fix while here:** `/f/feat-planner/engineering` logs console 404s because `concept/cited-pages.ts` prefetches summaries of plans whose committed overview cites a plan that no longer exists (`concerns/planner-ui-repair-followups.md`). The journey script fails on a console error, so this must go. The server's page already lists only plans with a concept page (`links.rs`), so drive the prefetch from that and do not fetch a citation that page cannot name.

**Keep:** `FoldCap`'s name `Fold`, `data-sentence-id`, the `j`/`k` behaviour, and every `concept/*.test.tsx` guard. Restyling must not change those tests, except `doc-tokens` values you did not touch.

One check: `node_modules/.bin/vitest run src/plan/concept src/plan/markdown`.

## P3: home and the remaining screens (sonnet)

Apply `DESIGN.md` to:

- `home/*` (cards, invitation, shelf, `home/progress-strip`, now six cells)
- `new/*` (kickoff)
- `settings/*`
- `help/*` scenes that drew the old surfaces
- `files/tree` chrome
- `header/activity.css`

Cards name the slug. The invitation stays. Keep the selector contracts in `help/features-scene.test.tsx` and run the help tests.

One check: `node_modules/.bin/vitest run src/plan/home src/plan/new src/plan/help`.

## Main agent

After the workers:

1. Run `node_modules/.bin/oxfmt`.
2. Build, then run the journey script and `scripts/plan-screens.sh --worktree-doc` over `/`, `/new`, `/f/feat-planner/product`, `/f/feat-planner/product/section_05`, `/f/feat-planner/engineering`, `/f/feat-planner/pressure`, `/f/feat-planner/run`, `/files/product/feat-planner/section_05.md` in all four themes.
3. Fix deviations through a fresh worker.
4. Run one `loom check pages-and-reading` beside the final `loom-code-reviewer` round, whose brief carries `DESIGN.md` and the screenshot paths.
