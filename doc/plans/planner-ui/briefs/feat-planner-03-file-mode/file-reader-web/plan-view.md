# file-reader-web / plan-view: plan header, Stages panel, Show YAML

Tier: sonnet, Claude lane. Starts after codex wave A (R1-R6). Read `doc/plans/briefs/feat-planner-03-file-mode/common.md` first.
Spec: `doc/product/section_06.md` "How plans render", "Plan validation" ("How errors show", the
rendered-view part). Design: `doc/loom/knowledge/conventions/web-dashboard-typography.md`.

## Files you own

`web/src/plan/files/plan/{plan-view.tsx, plan-header.tsx, validation-list.tsx, stages-panel.tsx, stage-row.tsx, stage-graph.tsx, plan-actions-extras.tsx}`
and their `*.test.tsx`. `split.ts` and `split.test.ts` are codex unit R4 (`codex-units.md`;
spec: item 2 below, `splitPlan(text, metadataLine) -> { prose, prose_offset, record, record_offset }`);
use it, never edit it.

## Rendering

`export function PlanView({ doc }: { doc: DocView }): JSX.Element` (`doc.plan` is set):

1. `PlanHeader`: state word, lock notice when locked, `Implements:` links to the product sections
   (`ImplementsLink.path` null -> struck through "not found"), last pressure test date beside
   `last_change`, and the validation result as "N errors, M warnings" that expands into
   `ValidationList` (errors, then warnings by bucket, then notes; each stage-specific item links to
   that stage's row: `#stage:<id>`). The header ends with
   `<PlanActionsExtras plan={doc.plan} path={doc.path} />` from `plan-actions-extras.tsx`, which
   returns null in plan 3 (doc comment: plan 11 adds "Pressure-test" for written plans and the
   done-plan actions there); an extension slot with a defined empty state.
2. The prose: `split.ts` cuts `doc.text` into prose before the metadata block, and the
   `## Pressure tests` record after the end marker, using `metadata_line` and the
   `<!-- END loom METADATA -->` line; both parts render with `<Markdown/>` (from `markdown`), the
   first with `lineOffset: 0`, the record with `lineOffset` = the number of file lines before it,
   so its `## Pressure tests` heading gets its real anchor. Test that anchor.
3. `StagesPanel` in place of the metadata block: `StageGraph` drawn with `layoutStages` from
   `web/src/lib/graph.ts` (read its signature and the `StageSummary` type it takes; map each
   `StageRow` to that shape with a neutral status and no live state) and the dashboard's React Flow
   setup (see how `web/src/components/graph/` renders it); then one `StageRow` per stage (id, name,
   type, summary, dependencies), each expanding to description, acceptance criteria and files as
   text; clicking a node scrolls to its row. A "Show YAML" control swaps the panel for
   `doc.plan.yaml` as a code block. Missing block: one line "No metadata block yet."; parse error:
   the error and the raw block.
4. The pressure-test record as Markdown.

Each stage row has `id="stage:<id>"` so `#stage:<id>` addresses work.

## Tests

split at the markers for a plan with and without a record; a plan with no block shows the one
line; a YAML error shows the error and the raw block; Show YAML toggles; clicking a graph node scrolls to its row; a stage row expands to description,
acceptance and files; the header shows the pressure date beside the last change date; an error with a stage id
links to `#stage:<id>`; Implements link to a missing section struck through.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/files/plan` pass.
