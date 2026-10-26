# plan-web-shell: codex units

Lane: `loom-codex-forwarder`, `--model gpt-6.1-terra --effort high`, foreground only, at most six
at once over disjoint files, each spawn with an explicit Bash timeout of 600000 ms. A unit is one
file, or one file plus its test, at most three steps, inside the wrapper's 540 s deadline; a
timed-out unit is re-split, never re-forwarded as is.

## Rules every unit follows (codex reads them from this file; the prompt is only the fixed line)

- Read `doc/plans/briefs/plan-server/common.md` and the spec section named below; write only the
  unit's files. Do not run git; touch no hidden loom state or work path. At most a compile check
  (`cd web && node_modules/.bin/tsc -b --noEmit`, or `cargo check --manifest-path loom/Cargo.toml`).
- Web traps: no TypeScript parameter properties (`erasableSyntaxOnly`); import `z` only from
  `@/api/zod`; leave 10% under the size limits. Codex does not run `oxfmt`: the orchestrator
  formats after every web unit (`cd web && node_modules/.bin/oxfmt <files>`).
- `loom map` answers from the base layer and cannot see files other units of this stage wrote;
  every shape a unit uses from another unit is pinned in its spec section.

## Orchestrator duties

**Module seeding (every stage):** whenever the orchestrator adds a `mod` line, it creates the
module's file at the same moment holding only a `//!` purpose line, so the crate always compiles
and every unit's proof selects its tests as soon as the unit writes them. The unit or worker that
owns the file replaces the seed; the seed is not ownership.


Add `#[cfg(test)] mod tests_schema;` to `loom/src/plan_server/wire/mod.rs` and `mod embedded;` to
`loom/src/plan_server/tests/mod.rs` before the units that own those files (small changes). After
every unit: `git status --short` (only the unit's files), format web files, and the proof command.

## Units

| Unit | Wave | Owns | Reads | Entry points | Spec | Proof (orchestrator) |
| ---- | ---- | ---- | ----- | ------------ | ---- | -------------------- |
| W1 | A | `web/src/plan/api/schema.ts`, `web/src/plan/api/schema.test.ts` | `loom/src/plan_server/wire/*.rs`, `web/src/plan/api/fixtures/*.json`, `web/src/api/schema.ts` (pattern) | the schemas and const arrays | api-state.md "schema.ts" | `cd web && node_modules/.bin/vitest run src/plan/api/schema.test.ts` |
| W2 | B | `web/src/plan/api/client.ts`, `web/src/plan/api/client.test.ts` | W1 (`ZodType` schemas), the atoms names pinned in api-state.md | `apiGet`, `apiPost`, `initClient`, `ApiResult` | api-state.md "client.ts" | `vitest run src/plan/api/client.test.ts` |
| W3 | B | `web/src/plan/api/socket.ts`, `web/src/plan/api/socket.test.ts` | W1, W2 signatures (pinned), `web/src/api/ws.ts` (pattern) | `connectPlanSocket`, `onResync`, `subscribe`, `send` | api-state.md "socket.ts" | `vitest run src/plan/api/socket.test.ts` |
| W4 | B | `web/src/plan/state/atoms.ts`, `web/src/plan/state/atoms.test.ts` | W1 types | every atom in api-state.md "atoms.ts" | api-state.md "atoms.ts" | `vitest run src/plan/state` |
| W5 | B | `loom/src/plan_server/wire/tests_schema.rs` | `loom/src/commands/status/web/schema_parity.rs:138` (`assert_all_in_schema`), W1's const arrays (pinned names) | `<enum>_wire_values_are_in_plan_schema` | api-state.md "schema.ts" (last paragraph) | `cargo test --manifest-path loom/Cargo.toml --lib plan_server::wire::tests_schema` |
| W6 | B | `web/scripts/bundle-check.mjs`, `web/scripts/bundle-check.test.mjs`, `web/scripts/bundle-rules.json` | none | the closure and rule functions, the CLI | entry.md "Bundle check" | `vitest run scripts` |
| W7 | B | `web/vite.config.ts` | `web/index.html`, the current `web/vite.config.ts` | `defineConfig(({ mode }) => ...)`: both inputs, `manifest: true`, the plan-mode proxy | entry.md "Vite" | `cd web && node_modules/.bin/vite build --outDir "$(mktemp -d "${TMPDIR:-/tmp}/w7.XXXXXX")" --emptyOutDir --logLevel error` (run after the shell worker returns, when `plan.html` and `main.tsx` exist) |
| W8 | C | `web/src/plan/offers/actions.ts`, `web/src/plan/offers/actions.test.ts` | W1 (`OfferView`, `OfferKind`) | `registerOfferAction`, `offerAction`, `runOffer` | shell.md "offers/actions.ts" | `vitest run src/plan/offers` |
| W9 | C | `web/src/plan/ui/disabled-reason.tsx`, `web/src/plan/ui/disabled-reason.test.tsx` | `web/src/components/ui/button.tsx` | `DisabledReason` | shell.md "ui/disabled-reason.tsx" | `vitest run src/plan/ui/disabled-reason.test.tsx` |
| W10 | C | `web/src/plan/ui/load-state.tsx`, `web/src/plan/ui/load-state.test.tsx` | `web/src/aurora-ui/feedback/EmptyState.tsx`, `HazardPanel.tsx`, `web/src/components/ui/skeleton.tsx` | `LoadState` | shell.md "ui/load-state.tsx" | `vitest run src/plan/ui/load-state.test.tsx` |
| W11 | D | `web/src/plan/router.tsx`, `web/src/plan/router.test.tsx` | `web/src/router.tsx`, the shell worker's `PlanShell` and `ChecklistView` (pinned in entry.md), W13's `PLAN_TAB_ROUTES` (pinned) | `routes`, `router` | entry.md "`web/plan.html`, `main.tsx`, `router.tsx`, `tabs.ts`" (router) | `cd web && node_modules/.bin/vitest run src/plan/router.test.tsx` (the index route renders `ChecklistView` inside `PlanShell`; a registered tab adds its route) |
| W13 | C | `web/src/plan/tabs.ts`, `web/src/plan/tabs.test.ts` | none | `PlanTab`, `PLAN_TABS`, `PLAN_TAB_ROUTES` | entry.md (tabs) | `vitest run src/plan/tabs.test.ts` |
| W12 | D | `loom/src/plan_server/tests/embedded.rs` | `loom/src/web_server/assets.rs` (`page`, `lookup`), the integrator's `tests/support.rs` | the three embedded tests | entry.md "`loom/src/plan_server/tests/embedded.rs`" | after the build: `cargo test --manifest-path loom/Cargo.toml --lib plan_server::tests::embedded` |

Waves: A (W1), then B (W2-W7, six at once), then C (W8-W10, W13) while the Claude `shell` worker starts
in the background once C has returned, then D (W11, W12) after the shell worker returns. `web/plan.html` and `web/src/plan/main.tsx` are
the shell worker's (Claude), not codex units.
