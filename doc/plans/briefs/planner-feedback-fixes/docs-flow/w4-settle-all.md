# W4: Settle all, server route and web button (codex sol, high)

Read `common.md` beside this file first. Decision D5 is authoritative.

## Goal

One press settles every product spec, or every plan, of a feature that needs settling, through the
same checks and commits a per-document Mark settled uses.

## Files you own

- `loom/src/plan_server/trace/mod.rs` (route registration and `mod` lines)
- `loom/src/plan_server/trace/settle_route.rs` (factor out the shared path)
- `loom/src/plan_server/trace/settle_all.rs` (new) and `loom/src/plan_server/trace/settle_all_tests.rs`
  (new; unit tests beyond the frozen `settle_all_contract_tests.rs`)
- `loom/src/plan_server/wire/trace.rs` (the response types)
- `web/src/plan/api/schema-trace.ts`
- `web/src/plan/trace/settle-all.tsx` (new) and `web/src/plan/trace/settle-all.test.tsx` (new)
- `web/src/plan/trace/settle-flow.ts`

## Steps

1. `settle_route.rs`: read `settle_route` from line 106 to its end, with `opened` and `refuse`.
   Factor the part after the hash check into `pub(super) fn settle_one(req, rel, target, text,
   confirm) -> Result<Saved, Refusal>`, so both routes run the same refusal, `settle::mark`,
   `save_document` and settle-queue job. `settle_route` keeps its exact responses; its tests in
   `route_tests.rs` must pass unchanged.
2. `settle_all.rs`: `pub fn settle_all_route(req: &ApiRequest<'_>) -> ApiResponse`.
   - Validate `feature` and `role`: only `section` or `plan`; anything else answers 400
     `bad_request`.
   - Walk `feature.trace.documents` with that role, settleable, and not `Settled`, in trace order.
     For each, compute the current hash the way `opened` does from the project content. Then call
     `settle_one` with `confirm` from the query.
   - Map refusals to `skipped` reasons: unresolved calls or open questions without confirm are
     `unresolved`, a trace that trails the file is `stale`, a stub is `stub`. One refusal never
     stops the rest.
   - Answer 200 with the shape pinned in `common.md`. Response structs live in `wire/trace.rs` with
     `serde::Serialize`.
   - Settle sections before plans. The route takes one role per call, so this holds by
     construction; say so in the rustdoc.
3. `mod.rs` `routes()`: add the `RouteSpec` (Post, `/api/settle-all`, `BodyKind::None`,
   `Role::HolderOrTake`, `settle_all::settle_all_route`), and update the rustdoc list above
   `routes()`.
4. Web, `schema-trace.ts`: `settleAllResultSchema = z.strictObject({ settled: z.array(z.string()),
   skipped: z.array(z.strictObject({ path: z.string(), reason: z.enum(["unresolved", "stale",
   "stub"]) })) })`.
5. Web, `settle-flow.ts`: add `postSettleAll(feature, role, confirm)` beside `postSettle` (lines 64
   to 75), using the same fetch helper and error handling.
6. Web, `settle-all.tsx`: `SettleAllButton({ feature, role })`.
   - Read the trace with the hook the spine uses (`useTrace`, `feature/spine.tsx:126`). Count the
     role's documents that are settleable and not settled. Render nothing at zero.
   - It is disabled with the reason from `authorBlockReasonAtom` the way `SettleControl` is
     (`trace/settle-control.tsx`).
   - On press, post. If the answer lists `unresolved` documents, show their count and a second
     button, "Settle anyway", which posts with confirm.
   - Use the existing button primitives and classes of `settle-control.tsx`; no new design.
   - Add tests: zero hides it, the count shows, unresolved offers the confirm.

## Traps

- The settle queue commits jobs one at a time in order (`settle_queue.rs`). Queue one job per
  document; never one combined commit.
- `Role::HolderOrTake`: copy the registration of `/api/settle` exactly.
- The route tests start a git-backed server in a TempDir. You cannot run them; the orchestrator
  will.

## Done

`cargo check --manifest-path loom/Cargo.toml --all-targets` and `cd web &&
node_modules/.bin/tsc -b --noEmit` are clean. Report the files changed and the exact exported
names.
