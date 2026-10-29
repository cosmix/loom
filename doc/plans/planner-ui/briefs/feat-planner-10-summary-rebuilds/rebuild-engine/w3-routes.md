# W3: message variants, registration, fixtures, concept-mode hooks (plus the rules C3 and C4 read)

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

Tier: codex sol/high (`loom-codex-forwarder`). Read `doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md`
("Wire", "Consumed interfaces") first. You run beside W2, after W1; you use W2's `Rebuilds` API
as common.md and the plan's CONTRACT SURFACE give it (build against it; if W2's file is not
complete when you build, wait and rebuild, never edit it).

Codex unit C3 writes `wire/summaries.rs` and `wire/tests_summaries.rs` from the "Wire" section and C4
writes `summaries/routes.rs` and `routes/tests.rs` from the "Routes" section; you own the rest.

## Files you own

`loom/src/plan_server/wire/mod.rs` (seeded; add `#[cfg(test)] mod tests_summaries;`),
`loom/src/plan_server/wire/message.rs` (four variants), `loom/src/plan_server/routes.rs` (two lines),
`loom/src/plan_server/concept/state.rs`, `loom/src/plan_server/concept/page.rs`, `loom/src/plan_server/concept/routes.rs`, and the three
fixtures `web/src/plan/api/fixtures/summary-{panel,question,turn-cost}.json`.

## Wire

`wire/summaries.rs`: the structs of common.md "Wire" with `Serialize, Deserialize`, `snake_case`,
token fields `skip_serializing_if = "Option::is_none"`. `wire/message.rs`: add
`SummaryPanel(SummaryPanel)`, `SummaryQuestion(SummaryQuestion)`,
`SummaryQuestionClosed { id: String }`, `SummaryTurnCost(SummaryTurnCost)` to `PlanMessage`.
Fixtures: hand-written realistic values (a running rebuild of 3, a last cost line with
`cache_write` absent, a quota snapshot with a `5h` and a `7d` window copied in shape from the
dashboard's quota fixture under `web/src/api/fixtures/`); `tests_summaries.rs` follows plan 2's
parity pattern (`<name>_fixture_matches_serde_output`, `<name>_fixture_deserializes`, enum values
checked against `web/src/plan/api/schema-summaries.ts` once the web stage writes it: in THIS stage
assert only the Rust round trip; note the schema check for stage `rebuild-web` in loom memory).

## Routes (`summaries/routes.rs`)

`pub fn routes() -> Vec<RouteSpec>`:
- `GET /api/summaries/panel`, `Role::Viewer` -> `Rebuilds::panel()`.
- `POST /api/summaries/rebuild`, JSON `{ ids: Vec<String> }` (each id passes
  `planning::summary::store::validate_id` and is in `expected_ids`; at most 64), `Role::Viewer`
  with the right checked in the handler: holder, or anyone while `ctx.authoring().holder()` is
  `None`; read-only mode -> 409 `read_only`. Taking authoring is NOT a side effect of pressing
  rebuild. -> 202 with the panel.
- `POST /api/summaries/release`, JSON `{ ids: Vec<String> }`, the same right as rebuild: releases
  terminal (null-session) claims listed in `SummaryPanel.terminal_claims` through
  `Rebuilds::release_terminal_claims`; an id that is not a terminal claim -> 409 `not_terminal`.
- `POST /api/summaries/answer`, JSON `{ question: String, rebuild: bool }`, the same right, mapped to
  `AnswerRight` -> 200 with the panel, 403 `not_allowed`, 404 `no_such_question`.
Register in `plan_server/routes.rs`: one line in `all_routes()` and one in `install_all()` placed
AFTER plan 5's concept install (W2's `install` needs `ConceptState` in `ctx.extensions()`). Concept routes are under `/api/concept/...`; these are `/api/summaries/...`.

## Concept-mode hooks (plan 5's names, confirmed by frontend)

- `concept/page.rs`: add `pub rebuilds: Option<Arc<Rebuilds>>` to `#[derive(Default)] PageContext`
  (every construction uses `..Default::default()`, the frozen plan 5 contract included; never remove
  that), and make `progress(ctx: &PageContext) -> (Option<Progress>, Option<String>)` return
  `Rebuilds::building()` and `Rebuilds::estimate_text()` (the never-built estimate reads
  "about <n> tokens to build, an estimate").
- `concept/routes.rs::page_context(ctx)`: set `rebuilds` from `ctx.extensions().get::<Rebuilds>()`.
- `concept/state.rs::out_of_date_reason(id: &str, state: &SummaryState, ctx: &PageContext) -> Option<OutOfDate>`:
  add the `awaiting_approval` and `failed` arms from `ctx.rebuilds` -> `out_of_date_reason(id)` to
  its one `match`; `rebuilding` stays from the claim state. No signature change.

## Tests (`routes/tests.rs`, and `concept/` unit tests)

`out_of_date_reason` returns `awaiting_approval` for an id in the open question and `failed` with
the error for a failed id, and `ConceptPage.building`/`estimate` are filled while a rebuild runs and
on a never-built project (extend plan 5's tests in `plan_server/concept/` beside the functions).

Route tests:

Each route's refusal (bad id, unknown id, 65 ids, not allowed while another browser holds, read-only)
through plan 2's route test helpers (the pattern plan 2's own route tests use; find it with
`rg -n 'fn .*_route_' loom/src/plan_server`).

## Proof

`cargo test --lib plan_server::wire::`, `cargo test --lib plan_server::summaries::`,
`cargo test --lib plan_server::concept::`, `cargo clippy --all-targets -- -D warnings`.
