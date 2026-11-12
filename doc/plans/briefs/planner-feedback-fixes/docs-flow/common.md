# docs-flow: shared context

Read this before your own brief. The plan's Decisions D1 to D5 are authoritative
(`doc/plans/PLAN-planner-feedback-fixes.md`, renamed `IN_PROGRESS-` once the run starts).

## Rules for every worker

- Write only the files your brief owns. Another worker owns every other file. If a file you do not
  own must change, stop and report it.
- No git commands. Never touch any `.loom/` path.
- Codex workers: compile only (`cargo check --manifest-path loom/Cargo.toml --all-targets`, and
  `cd web && node_modules/.bin/tsc -b --noEmit`). Run focused vitest files if they run. The
  orchestrator runs the fixture-backed Rust tests after you. If the build fails in a file you do not
  own, another worker of the same wave is mid-edit: report it and do not touch that file.
- Size limits: file 400 lines, function 50 lines. Leave a 10% margin, because a formatter can push
  a file over.
- TypeScript: no parameter properties (`erasableSyntaxOnly`). Strict zod (`z.strictObject`) means a
  new wire field must appear in the Rust serializer, the zod schema and every JSON fixture together.
- Rust: literal braces inside `format!` are doubled. A CLI placeholder in a rustdoc comment goes in
  backticks.
- Delete what your change orphans: functions, CSS rules, test helpers and fixture keys. Never leave
  a stub.
- The project is unreleased: no backwards compatibility and no migration code.
- No mention of AI tools in code, comments or docs.

## Pinned interfaces

### Engineering overview removal (D2)

- The summary id `overview/<f>/engineering` no longer exists anywhere. `overview/<f>/product`
  stays.
- `Scope::Engineering` (Rust) and the `engineering/:doc` web route stay; they are the per-plan
  summary pages. Only the overview of that scope goes.
- Web page names: the Plans group targets the page `metaplan`. The route `engineering` (no
  `:doc`) and `run` both render `<Navigate to="../metaplan" replace />`.

### Metaplan gate (D1)

- `pub fn metaplan_settled(feature: &Feature) -> bool` in `loom/src/planning/trace/gate.rs`
  (W3). It returns true when the feature's trace document with role metaplan has settle state
  `Settled`, or is not settleable; false when it has no metaplan.
- `StartError::MetaplanUnsettled`, displayed as `Settle the metaplan first`.
- The wire `OfferView` gains `blocked_reason: Option<String>`, serialized always, `null` when
  open. Zod: `blocked_reason: z.string().nullable()`. It is set to `"Settle the metaplan first"` on
  `write_plan` and `draft_all_plans` offers while the gate is closed. `OfferButton` disables itself
  and shows the reason through `DisabledReason` when it is non-null.

### Settle all (D5)

- Route `POST /api/settle-all?feature=<f>&role=section|plan[&confirm=1]`, `Role::HolderOrTake`,
  `BodyKind::None`, handler `settle_all::settle_all_route` (W4).
- Answer: 200 `{"settled": [rel], "skipped": [{"path": rel, "reason": "unresolved"|"stale"|"stub"}]}`.
  A missing feature or a role other than section or plan answers 400 `bad_request`.
- Web: `export function SettleAllButton({ feature, role }: { feature: string; role: "section" | "plan" })`
  in `web/src/plan/trace/settle-all.tsx` (W4). It renders nothing when no document of that role
  needs settling. Its label is "Settle all specs" for section and "Settle all plans" for plan.
  W6a mounts it: role section in the product overview's `above`, role plan on the metaplan page.

### Spine (D3, D4)

- `SpineGroup.glyph` for product and plans is `{ kind: "attention", word: "Not all settled" }` when
  the server mark is `done` and any child glyph kind is not settled, done or retired; otherwise
  `null` (W5).
- `SpineGroup.overview` is removed for plans. The product overview child stays.
- A running plan row draws `LiveDot`, not `BusyRoundel`.

## Tests

Update the tests that assert removed behaviour. Each such edit is an expected integrity event
listed in the stage's INTEGRITY line; never weaken an assertion about behaviour that stays. Every
new behaviour gets a focused test in your territory. Contract files (`*_contract_tests.rs`,
`*.contracts.test.ts[x]` added by the contract session) are frozen: never edit them.
