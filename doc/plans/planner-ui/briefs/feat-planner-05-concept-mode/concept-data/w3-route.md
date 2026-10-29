# concept-data W3: engine driver, routes, delta, registration

Working dir `loom/`. You own `src/plan_server/concept/{mod.rs, engine.rs, routes.rs,
route_tests.rs}` and two lines in plan 2's `src/plan_server/routes.rs` (one in `all_routes()`,
one in `install_all()`). Read plan 2's existing route modules first (`plan_server/<name>/mod.rs`
exposing `routes()` and `install`) and copy their shape, including how their route tests drive a
handler. W1's and W2's files exist when you need them.

## mod.rs

Keep the module declarations the foundation wrote; add the re-exports `pub use
engine::{install, ConceptState}; pub use page::{load_page, PageContext, PageKey, Scope}; pub use
routes::routes;` once the items exist.

## engine.rs

`pub struct ConceptState` stored in `ctx.extensions()`: the `SummaryEngine` behind a `Mutex`,
the latest `Arc<Evaluation>` behind an `RwLock`, the previous batch's per-summary states and file
hashes (for the delta), and listeners `Vec<Box<dyn Fn(&PlanContext, &EngineUpdate) + Send +
Sync>>`. `pub fn latest(&self) -> Arc<Evaluation>`; `pub fn subscribe(&self, f)` (plan 10 calls
it). `pub fn install(ctx: &Arc<PlanContext>)`:

1. Build the engine with `SummaryEngine::new(ctx.root().to_path_buf(),
   ctx.runtime().clone(), <the watcher's OwnWrites>)`: the orchestrator tells you how plan 2
   exposes the watcher's `OwnWrites`. Seed `latest` with `evaluate(&project, &list_claims(..))`.
2. `ctx.on_batch(Box::new(...))`: on each batch call `on_batch(&project, batch)`; on error log one
   terminal line through `plan_server::lifecycle::line` and keep the previous evaluation; store
   the new evaluation; call every listener with the `EngineUpdate`; compute and broadcast the
   delta (below).

## The delta

`pub fn page_keys_for_summary(id: &str, project: &Project) -> Vec<String>` (re-export it from
`mod.rs`): `product/section_04` gives `["product/section_04", "product"]` when the section is in
the product overview's scope, `plans/<slug>` gives `["engineering/<slug>", "engineering"]` (no
overview key when outside scope), `product` / `engineering` give themselves. The delta below uses
it; plan 10 calls it too. Test "page keys for a summary".

After each batch compare per-summary state and the summary file's hash with the previous
batch's: a summary whose state or file changed puts its page key in `pages`, and a document
summary that changed also puts its scope's overview (`product` or `engineering`) there; a
summary that became `Stale`, `Incomplete` or `Rebuilding` puts its id in `out_of_date`; every
batch path under `doc/` outside `doc/loom/summaries/` goes in `sources`. Broadcast
`PlanMessage::ConceptChanged(ConceptChanged { pages, out_of_date, sources })` through
`ctx.hub().broadcast`; broadcast nothing when all three lists are empty.

## routes.rs

`pub fn routes() -> Vec<RouteSpec>`: one `RouteSpec { method: Get, path: "/api/concept/" (prefix),
body: None, role: Viewer, handler }`. The handler takes the path after `/api/concept/`,
`PageKey::parse` (None: 400 `{"error":"bad_page"}` through `ApiResponse::error`), reads
`ctx.project()` and `ConceptState::latest()`, builds `PageContext { read_only, ..Default::default() }`
from `ctx.read_only()` (the first problem's message, or `None`) in one function
`page_context(ctx)` that plan 10 extends, and answers `load_page` as JSON
(None: 404). Register with `concept::routes()` in `all_routes()` and `concept::install(` in
`install_all()` (the stage's wiring checks grep both texts in `routes.rs`).

## route_tests.rs

Through plan 2's route-test harness: overview 200 with chips; unknown section 404;
`product/../x` 400; a POST is refused by the router (405 or plan 2's refusal); one delta test
feeding two batches (a source edit, then nothing) and asserting one `concept_changed` then none;
a listener registered with `subscribe` receives the `EngineUpdate`. Proof:
`cargo test --lib plan_server::concept::`.

## Required test cases (coverage audit; named tests in `route_tests.rs`)

"overview 200 with chips"; "viewer gets 200" (no role gate: read-only browsers see the same
pages); "unknown section 404"; "dot-dot key 400"; "post refused"; "delta once then none";
"subscribe receives EngineUpdate"; "engine error keeps the previous evaluation".
