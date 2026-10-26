# Stage `checklist-check`: common brief

Read `doc/plans/briefs/feat-planner-01-documents/common.md` first (the authoritative API). This file fixes
the checklist rules section 01 leaves to the implementation, the convention-problem rules of
section 09, and the JSON the contracts pin. Stage `document-model` is merged: `loom::planning`
already has `kinds`, `slug`, `discover`, `runtime`, `sections`, `paths`, `watch`.

## The checklist (section 01), decided

Order ("first"): product sections in the vision table's row order; plans by metaplan (metaplans
sorted by path) then entry heading order; unlisted plans by file name.

Step marks (`StepMark`):

| Step | Mark |
| --- | --- |
| 1 Vision | `Done` when `doc/product/index.md` exists, else `Gap` |
| 2 Product spec | `Done` when the vision's table parses and every listed section file exists and holds a non-whitespace character (an empty or blank-only file is a gap); otherwise `Gap` (a missing vision or an unreadable table is a gap of step 2 as well) |
| 3 Metaplan | `Done` when at least one metaplan exists, else `Gap` |
| 4 Plans | `Done` when a metaplan exists and every entry's slug resolves to a plan in any state; otherwise `Gap` |
| 5 Pressure test | `Optional` while any plan in state `Written` has no pressure record; otherwise `Done` |
| 6 Review | `Optional` always (never recorded) |
| 7 Run | `Active` while any plan is `Running`; `Done` when at least one metaplan entry exists and every entry's plan is `Done` or `Retired`; otherwise `Optional` |

Current step: the earliest of steps 1 to 4 with mark `Gap`; if none, `Step(PressureTest)` while
step 5 is `Optional`; else `ReadyAllDone` when step 7 is `Done`; else `Ready`.

Items (`ChecklistItem { step: Step, status: ItemStatus, path: Option<PathBuf>, label: String, offer: Option<Offer> }`,
`ItemStatus::{Present, Gap, Unlisted, Untested}`): one per expected document (vision; each listed
section; each metaplan, or one `Gap` item for "no metaplan"; each metaplan entry), one `Unlisted`
item per unlisted section and unlisted plan, one `Untested` item per written plan without a record.
A vision with no readable table yields one `Gap` item "the vision lists no sections in a form the
app can read" with offer `FixVisionTable`.

Offers:

| Current | Primary | Secondary (in this order, each once) |
| --- | --- | --- |
| Step 1 | `StartVision` | every other gap's offer in step order |
| Step 2 | `FixVisionTable` when the table is unreadable, else `WriteSection { lowest missing in table order }` | the other gaps' offers |
| Step 3 | `StartMetaplan` | the other gaps' offers |
| Step 4 | `WritePlan { first missing entry }` | the other gaps' offers |
| Step 5 | `PressureTest { first untested written plan }` | `StartRun`, the other `PressureTest` offers |
| Ready | `StartRun` | `RefineSpec` |
| ReadyAllDone | `RefineSpec` | `NewMetaplanRound` |

In every state the secondaries also carry: `PressureTest` for every untested written plan not
already offered (so a project with gaps still offers tests, section 01 "Plans with no metaplan");
`AddSectionToVision { n }` for each unlisted section when a vision exists; `AddPlanToMetaplan`
for each unlisted plan when a metaplan exists. With no vision, unlisted sections get no offer of
their own (the `StartVision` offer lists them). `StartRun` appears only once steps 1 to 4 have no
gap. Read-only browsers drop offers in the app (plan 2); the library always computes them.

`Offer::id()`: `start-vision`, `fix-vision-table`, `write-section:<nn>`,
`add-section-to-vision:<nn>`, `start-metaplan`, `write-plan:<slug>`, `add-plan-to-metaplan:<slug>`,
`pressure-test:<slug>`, `start-run`, `refine-spec`, `new-metaplan-round` (`<nn>` two-digit).
`Offer::target()`: the section file for section offers, the plan's current path for plan offers
(`WritePlan` targets `doc/plans/PLAN-<slug>.md`), the vision for `FixVisionTable`, else `None`.

## Convention problems (section 09), decided

Reported only for documents that are not locked (a locked plan, its briefs, a run review and the
archive cannot be edited, so a problem there could never be fixed); `stray-plan-copy` is the
exception and is reported on every stray.

| Code | When | Path, line |
| --- | --- | --- |
| `doc-missing` | `doc/` is missing or a symlink | `doc`, null |
| `vision-table-unreadable` | the vision exists and has no `Specification sections` heading with a table holding at least one row whose first cell is a number | the vision, the heading's line or null |
| `unlisted-section` | `doc/product/section_<nn>.md` not in the table (also when there is no vision) | the section, null |
| `missing-title` | a vision, product section, metaplan or plan whose first non-blank line is not a level-1 heading | the file, 1 |
| `metaplan-entry-without-plan` | a level-2 heading in a metaplan whose text starts with a number and `.` (`## 5. Concept mode`), names no plan file in backticks, and does not say `no plan file` (`## 6. Policy notes (no plan file)` is prose) | the metaplan, heading line |
| `implements-missing` | a `Written` plan with no `Implements:` line (running and later plans are locked) | the plan, null |
| `implements-duplicate` | a second `Implements:` line | the plan, that line |
| `implements-broken` | a bad token, or a section number the vision's table does not list | the plan, the line |
| `broken-reference` | a cross-reference (section 03's three forms) that does not resolve to a discovered document, or names an anchor the target lacks | the document, the line |
| `broken-brief-reference` | an entry of `collect_inputs(...).invalid`, or an input file that does not exist | the plan, null |
| `stray-plan-copy` | every `PlanEntry::strays` path | the stray, null |

Lines are 1-based in `Problem.line` (the API's spans are 0-based; convert at the boundary).
Problems are sorted by path, then line, then code.

## JSON (`loom plan check --json`)

```json
{
  "schema": 1,
  "root": "/abs/repo",
  "checklist": {
    "current": "vision",
    "steps": [{ "step": "vision", "mark": "gap" }],
    "items": [{ "step": "plans", "status": "unlisted", "path": "doc/plans/PLAN-alpha.md", "label": "...", "offer": { "id": "pressure-test:alpha", "kind": "pressure-test", "slug": "alpha", "target": "doc/plans/PLAN-alpha.md" } }],
    "primary": { "id": "start-vision", "kind": "start-vision", "target": null },
    "secondary": [],
    "state": { "vision_present": false, "sections_listed": 0, "sections_written": [], "metaplans": [], "plans_unlisted": 3, "plans_by_state": { "written": 2, "running": 1 }, "locked_files": ["doc/plans/IN_PROGRESS-PLAN-gamma.md"] }
  },
  "problems": [{ "code": "implements-missing", "path": "doc/plans/PLAN-x.md", "line": null, "message": "..." }]
}
```

`current` is one string: `vision`, `product_spec`, `metaplan`, `plans`, `pressure_test`, `ready`,
`ready_all_done`. `steps` always has 7 entries in step order with `step` in snake_case
(`review`, `run` included) and `mark` in snake_case. An offer serialises as an object with `id`,
`kind` (kebab-case variant name), `target` and its variant fields (`number`, `slug`, `metaplan`).

## Schema fixture

`loom/tests/fixtures/planning/check_schema.json` (written by the contract session, frozen) is a
PROJECTION of the output for the fixture "plans with no product spec": a git repository whose
`doc/` holds only `doc/plans/PLAN-alpha.md` (`# Plan: Alpha`, a blank line, `Implements: none`),
`doc/plans/PLAN-beta.md` (`# Plan: Beta`, no `Implements:` line) and
`doc/plans/IN_PROGRESS-PLAN-gamma.md` (`# Plan: Gamma`, no `Implements:` line; locked, so it
reports no problem). The projection
is `{"schema", "current", "steps", "primary": <primary.id>, "secondary": [<ids>], "problems": [{"code","path","line"}]}`;
for that tree it is:

```json
{
  "schema": 1,
  "current": "vision",
  "steps": [
    { "step": "vision", "mark": "gap" },
    { "step": "product_spec", "mark": "gap" },
    { "step": "metaplan", "mark": "gap" },
    { "step": "plans", "mark": "gap" },
    { "step": "pressure_test", "mark": "optional" },
    { "step": "review", "mark": "optional" },
    { "step": "run", "mark": "active" }
  ],
  "primary": "start-vision",
  "secondary": ["start-metaplan", "pressure-test:alpha", "pressure-test:beta"],
  "problems": [
    { "code": "implements-missing", "path": "doc/plans/PLAN-beta.md", "line": null }
  ]
}
```

Secondary order there: the other gaps' offers in step order (step 2 has no offer of its own
without a vision; step 3 `start-metaplan`; step 4 has no entry to write without a metaplan), then
`pressure-test:alpha`, `pressure-test:beta` (file-name order). The contract builds the same projection from the real output and compares
with `serde_json::Value` equality.
