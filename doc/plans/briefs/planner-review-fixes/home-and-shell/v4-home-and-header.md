# V4 — Home redesigned from scratch, and the header (issues 1, 2, 15)

Tier: opus, effort medium (`loom-senior-software-engineer`). This is the operator's named "redesign from scratch". First invoke `frontend-design:frontend-design` with the Skill tool. Then read `common.md` (Design rules, Visual work), `web/src/plan/ui/DESIGN.md`, `doc/loom/knowledge/architecture/planner-web-shell.md` ("Home and kickoff", "Header and dock"), and this brief.

## Home (issue 1)

Today (`web/src/plan/home/home.tsx`, `standalone-shelf.tsx`, `standalone-shelf.css`, `move-into-feature.tsx`):

- standalone plans are wrapped chips capped at 24 (`CHIPS_SHOWN`) with a "+N more" button;
- "Add plan" navigates to `/new` with `state.small`;
- "Move plan" opens a radio list of every plan, so only one moves;
- a move is a chat session started with a text built by `beginMove` (move-into-feature.tsx 34-49), and there is no move API.

Fixed targets:

- **Standalone plans as a table** inside the existing collapsed `details`.
  - Frozen `home/features.contracts.test.tsx`: region "Standalone plans", a `details` closed by default, a summary containing "<n> standalone plans", plan links `/files/plans/…` inside the details and outside the summary.
  - Columns: a select checkbox; Plan (title, as a link to the plan; slug in mono under it); State (glyph and word); and nothing else. The wire `StandalonePlanView` carries only `{slug, path, state, title}` (`api/schema.ts:163-168`).
  - Every plan is listed, with no cap. The header row can sort by title or state, and a filter field narrows by title or slug at 10 plans and above.
  - A header checkbox selects all visible plans and shows the indeterminate state.
  - At 390px the table becomes a stacked list with the checkbox kept, and never scrolls sideways.
- **Selection actions.** With one or more plans selected, a quiet action bar reads "<n> selected · Move to a feature… · Clear".
  - Accessible names the stage contract `standalone-plans-move-together` uses: each row checkbox `aria-label="Select <title>"`; the action-bar button "Move to a feature…"; each destination a radio named by the feature slug; the dialog's confirm button named exactly "Move" (it reads "Move into feature" today: update and name every test that looks for the old name).
  - Move opens the move dialog for the selected plans (`move-into-feature.tsx`): the destination is an existing feature or a new name (`isFeatureName`), and the list shows the selected plans rather than a radio of all plans.
  - `beginMove` takes several paths and builds ONE message: `Move these plans into the feature "<name>": <repo paths, comma-separated>. Use git mv into doc/plans/<name>/, keep file names, and fix every reference to them.`
  - The `Offer::MoveIntoFeature` rows for visions, sections and metaplans (`movesAtom`, standalone-shelf.tsx 65-86) keep their one-path move.
- **Add plan.** One clearly labelled "New standalone plan" action in the shelf's summary row. It navigates to `/new` with `state: { small: true }`, and /new shows "Standalone plan" selected (kickoff stage, D13). Keep a unit test that the action navigates with that state (`standalone-shelf.test.tsx` has one today, about line 143).
- **Layout.**
  - The invitation, the Unfinished shelf (kickoff stage's `home/drafts-shelf.tsx`) and the feature cards keep their order and their names: "Start a conversation" and the feature card links named by slug.
  - `<DraftsShelf />` stays mounted in `home/home.tsx`: the kickoff stage's wiring check greps `<DraftsShelf` there, and integration-verify re-runs it on the merged tree.
  - Redesign their spacing and hierarchy so Home reads as one calm page.

## Header (issue 15, D17)

`web/src/plan/header/header.tsx`, `header.css`, `feature-switcher.tsx`, `tabs.tsx`, `overflow.tsx`; the reference is the dashboard header, `web/src/components/header.tsx:29-81`.

- **Switcher dropdown** lists features only. Remove the "Home" (`<Link to="/" …>`) and "New work…" entries from `feature-switcher.tsx` (the plan's `after_stage` greps for both) and update the `feature-switcher.test.tsx` cases that expect them.
  - On `/new` the identity shows the live kickoff's name from `kickoffAtom.shape.name` (the kickoff stage applies `name_override` there), else "Untitled". Update the `header.test.tsx` case that pins the `/new` identity.
- **Tabs stay.** The Home and Files tabs in `tabs.tsx` stay: the frozen `feature/spine.contracts.test.tsx` "the header has no rail or view tabs" asserts links named exactly "Home" and "Files". The logo stays a home link named "Loom planner home" (journeys). Restyle the tabs; keep their names.
- **Spacing:**
  - 16px block padding: `.plan-appbar` in `header.css` uses `var(--space-4)` (16px) where it uses `var(--space-2)` (8px) today (the plan's `after_stage` checks the old rule is gone);
  - the logo at 32/40/56px (frozen `header/activity-fixes.contracts.test.tsx`);
  - at 1024px and up, at least 12px between the logo's bottom and the header's bottom border;
  - controls on one centre line.
  - The dashboard's `bg-linear-to-b from-card to-background` wash is allowed. Add it to DESIGN.md Surfaces as an exception that matches the dashboard.
- **Switcher look:**
  - the eyebrow ("Planning · feature") above the slug;
  - the slug in `--type-title`;
  - a chevron that reads as a menu;
  - one hover surface;
  - the popover list with the current feature marked by `aria-current` and a glyph.
- **Footer:** H1 builds `shell-footer.tsx` as `<footer className="plan-footer">`. You style `.plan-footer` in `ui/design.css` so it pairs with the header: a hairline top border, `--type-fine`, and the page gutter.

## DESIGN.md

You are its only editor in this stage. Add your exceptions (the header wash) and those H1 and V5 report, which the main agent passes you.

## Files

Yours are listed in the plan YAML worker table. `share.tsx` and `share.test.tsx` are H2's.

## Run once at the end

`node_modules/.bin/vitest run src/plan/home src/plan/header src/plan/feature/spine.contracts.test.tsx src/plan/help` from `web/`.
