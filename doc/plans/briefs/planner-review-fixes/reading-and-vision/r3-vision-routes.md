# R3 — vision-first landing, the vision summary page, source inside the feature (issues 3, 6)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first (D1, D2 and D4). R1 builds the server side in parallel. Work from the page key and paths in D1; the wire shape of a summary page does not change.

## Landing (D2)

- `web/src/plan/feature/landing.ts` `landingPath`: a vision present gives `vision`. No vision with a metaplan gives `engineering`. Neither gives `vision` (its "start vision" offer).
- Flip every `feature/landing.test.ts` case that expects `/product`, including "vision lists no sections yet".
- `feature/spine.contracts.test.tsx`, your two planned frozen edits (common.md "Planned edits of frozen tests"; nothing else in the file changes):
  - replace `it("finished feature opens the product overview", …)` with `it("a feature opens its vision", …)`. Keep its body except the expected pathname (`/f/atlas/vision`) and the content check, which reads the vision page's text.
  - "the Vision row opens the vision document": keep the href and pathname assertions; make `stubServer` answer concept key `atlas/product/vision` with a summary page, and replace `findByText(VISION_BODY)` and `fetchedDocs` with a check for that page's first sentence.
- `concept/concept.contracts.test.tsx`, your third planned frozen edit: in "concept opening a summary sentence shows its source passages as inset blocks", only `startsWith("/files/product/demo/section_01.md")` becomes `startsWith("/f/demo/source/product/demo/section_01.md")`.
- `web/scripts/plan-journeys.mjs` is 407 lines at base, over the limit. Move `j1` and the J2 views (`j2Views` and their helpers) into a new `web/scripts/plan-journeys-reading.mjs`, imported the way `plan-journeys.mjs` imports `j4, j5` from `plan-journeys-chat.mjs`, so every journey file ends at or under 400 lines. In J1, wait for `ROOT/vision` instead of `ROOT/product`.

## Vision summary page (D1)

- `web/src/plan/router.tsx`, the `vision` child route: it keeps `VisionPage` as its element.
- `web/src/plan/feature/document-page.tsx` `VisionPage`: `!checklist.has_product` still renders `MissingDocument` with the start_vision offer (a feature with no vision must keep its way to start one, and D2 now lands such a feature on `/vision`). Otherwise it renders `ConceptRoute` for the vision's summary page instead of `SourcePage`. Add a router test for each branch. `SourcePage` stays for the metaplan and for the source route below.
- `web/src/plan/api/page-key.ts`:
  - `parsePageKey` and `pageKeyOfPath`: `/f/<f>/vision` maps to the summary key `<f>/product/vision`.
  - `docKindOfPageKey` returns `"vision"` for it.
  - Update `page-key.test.ts`.
- `web/src/plan/files/doc-heading-view.tsx` `summaryTarget` and `files/section-chips.ts`: the vision's summary is now the vision page, not the product overview.
- `web/src/plan/feature/spine-model.ts`: the Vision row still links `/f/<f>/vision` (frozen spine contract).
- Empty state: a vision whose summary is not built yet shows the server's empty page (R1), with Source one click away.

## Source inside the feature (D4)

- Add a child route `{ path: "source/*", … }` under `f/:feature` in `router.tsx`. It renders `SourcePage` (`feature/document-page.tsx`) for `doc/<splat>`, inside `FeatureLayout`, so the spine stays.
  - It refuses (with the existing `LoadState` error) a path that does not belong to the feature: its vision, `doc/product/<f>/**` or `doc/plans/<f>/**`.
- Add `featureSourceHref(feature, repoPath, opts?: {lines?, anchor?})` in a new `web/src/plan/feature/source-href.ts`, with a test.
- Use it where a feature page links to a feature document's source:
  - the summary page Source view (`concept/page-heading.tsx` `sourceTo`, built from `page.path`);
  - passage chips (`concept/passage.tsx` `passageHref`). It takes no feature today: derive the feature from the path (`doc/product/<f>/…` or `doc/plans/<f>/…`), and keep `toAddress` for a path outside a feature;
  - `files/heading-parts.tsx` `featureCrumb` stays a link to `/f/<f>`.
  - Standalone plans and documents outside a feature keep `toAddress` (`files/address.ts`) and `/files`. The frozen home contract needs `/files/plans/…` links.
- On the source route the heading's Summary link goes back to the document's summary page (`summaryTarget`), and the spine marks the document's row as current (`spine-model.ts` `groupOfPathname`: a `/source/` path selects the group of its document).
- Edit stays `/files/…?edit=1` (the editor lives in file mode).
- J2 (now in `plan-journeys-reading.mjs`) expects Source to land on `/files/`. Make it expect `/f/feat-planner/source/`, and check the spine is still present (`nav[aria-label="Feature documents"]`).

## Tests

- The frozen stage contracts `feature-opens-on-vision`, `source-keeps-the-spine` and `vision-page-is-a-summary`.
- Unit tests for `landingPath`, `featureSourceHref`, the route's refusal of a foreign path, and both `VisionPage` branches.
- A `passage.test.tsx` case: a passage chip for `doc/product/atlas/section_01.md` links `/f/atlas/source/product/atlas/section_01.md?lines=…#<anchor>`, and one for `doc/plans/PLAN-x.md` keeps `/files/plans/PLAN-x.md`.
- Known `/files/` expectations your links change, all yours: `concept/chain.test.tsx` (pathname after clicking a passage header), `concept/concept-route-headers.test.tsx` (the Source href), and `files/doc-heading-view-parts.test.tsx` if the open-questions chip moves. `rg` for any other `/files/` expectation in `src/plan/concept`, `src/plan/feature` and `src/plan/files` tests and update it. Report each by file and test name.

V1 restyles the heading and spine in parallel. You own the routing and link logic in the files listed in the plan YAML; V1 owns their CSS and the presentational components. Report any change you need in a V1 file.

## Run once at the end

`node_modules/.bin/vitest run src/plan/feature src/plan/concept src/plan/files src/plan/api` from `web/`.
