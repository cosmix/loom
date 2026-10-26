# Plan `plan-summary-rebuilds`: common brief

Read this first, then your own brief. Plans 1 to 9 are merged when this plan runs. This file
fixes the scheduler rules, the cost model and the wire shapes, and lists every sibling interface the
plan consumes.

## The spec you implement

`doc/product/section_04.md`: "Who rebuilds what" (Background; the last two sentences of
Refinement: no background start during an authoring turn, the turn-end handoff), "Sealing a
summary" ("Clearing claims"), "Asking before a large rebuild", "Cost", "In read-only mode".
`doc/product/section_05.md` "Sentences and their marks" (out-of-date reasons) and "Empty and
partial states" (building progress, cost line, the approval notice). `doc/product/section_07.md`
"Stopping `loom plan`", the summary-session sentence after it, and "Crashes and kills".

## Scheduler rules (decided)

| Input | Rule |
| --- | --- |
| Watcher batch (`EngineUpdate`) | `fresh = invalidated - declined - running ids - queued ids`. Add `fresh` to the queue. Then `try_start`. |
| `try_start` | Nothing when read-only, suspended (any guard, including an authoring turn), a session is running, or a question is open. Otherwise: queue size <= `limit` (5) or every queued id came from a Button -> start one session with the whole queue; else open one question with the queue (count, ids, estimate). |
| Question answered `rebuild=true` | Close it; start with its ids (no second check). |
| Question answered `rebuild=false` | Close it; its ids join `declined` and leave the queue. |
| Who may answer | `AnswerRight::Holder`, or `FreeAuthoring` while nobody holds authoring; `NotAllowed` is refused and the question stays open. |
| Button (`enqueue(ids, Button)`) | Remove the ids from `declined`, put them at the queue's front marked Button, `try_start` (no question). |
| Summary becomes Current | Drop it from `declined`, the queue and `failed`. |
| Turn started (authoring session) | Hold an `AuthoringTurn` suspension. |
| Turn ended | Release it; queue every id still out of date (from the latest evaluation) minus `declined`; `try_start`. |
| Session running | Ids invalidated meanwhile wait in the queue as the next batch. |
| Claims of the running session | Track the ids it has claimed; when it has claimed at least one and now holds none -> stop it (claims are sealed or released). |
| Seal seen (a summary file of a running id changed and evaluates non-missing) | Reset the stall timer. |
| Stall (`NO_SEAL_TIMEOUT` = 10 min since start or last seal) | Stop with `release_claims = true`; the ids it held and did not seal go to `failed` with the last seal error (else "no seal in 10 minutes"). |
| Session ended (`BridgeEvent::Ended`, any outcome) | Clear its claims (`clear_session_claims`); unsealed ids go back to the queue, except on `Failed(reason)`, where they go to `failed` with the reason; `try_start`. |
| Authoring session goes Idle or Ended with a turn still open (Claude Code may write no Stop after an interrupt) | Release the `AuthoringTurn` suspension exactly as `TurnEnded` would (`authoring_state_idle`); a later `TurnEnded` for that turn is ignored. |
| Start-up, after plan 7's recovery (`Bridge::on_recovered`, which runs at once when recovery is already done; recovery's own `Ended` events fire before plan 10 subscribes) | `Attribution::sweep_after_recovery(live, left_alone)`: delete every claim whose `session` is set and is neither a live bridge session nor one recovery left alone; null-session (terminal) claims stay and are listed in the panel with a release action (`POST /api/summaries/release`, holder or free). |
| Every 5 s tick while a summary session runs | Re-read `claims::list_claims` for it (claims live in the runtime dir, which the watcher does not walk) and call `claims_changed`. |
| Any event that changes `out_of_date_reason` or `building` for an id (question opened/closed, started, claims changed, failed, stopped) | Broadcast `PlanMessage::ConceptChanged(plan_server::wire::concept::ConceptChanged { pages, out_of_date: ids, sources: vec![] })` with `pages` = the union of `plan_server::concept::page_keys_for_summary(id, &project)` (plan 5: the summary's own page plus its scope's overview). |
| `start_summary` fails (plan 8's `StartBlocks` makes `render_session_guidance` return Err while plan 11 holds it; any spawn error) | Start nothing, keep the ids queued, show the reason in the panel (`SummaryPanel.start_error`), retry at the next batch or resume. |
| Server stopping (first Ctrl-C) | Drop the queue and any open question; a running session gets plan 7's bounded end (plan 7 owns it); start nothing more. |

`failed` ids are not retried automatically; a Button press or a later change that invalidates them
again queues them.

## Cost model (decided)

- Tokens per transcript: `tokens::accumulate::AgentState::new(AgentKind::Main | AgentKind::Subagent, path)`
  and `ingest(line)` for every complete line (`loom/src/tokens/accumulate.rs:87`, `:117`); read
  `state.total` (`TokenUsage { input, cache_write, cache_read, output }`, `tokens/model.rs:28`).
  `TokenVector` maps input to `fresh_input`. A transcript that cannot be read gives every field `None`.
- Attribution: a subagent's transcript (`Bridge::transcripts`) holds its Bash `tool_use` blocks;
  a `loom plan summary brief <ids>` call binds those ids to that subagent. When it stops
  (`SubagentStopped`): one `CostLine` per id when it briefed exactly one id (its transcript's
  tokens), else one line for the set. The background session's own transcript is one line for the
  overview ids it sealed itself, level overview. Refinement subagents of the authoring session are
  costed the same way with trigger Refinement, and their ids feed `Bridge::set_rebuilding` for the
  open turn and a `SummaryTurnCost` message when costed.
- Log: `<runtime dir>/summary-cost.jsonl`, one JSON object per line, appended under an fs2 lock on
  the log file opened `O_APPEND | O_NOFOLLOW`. Unknown token fields are omitted (`skip_serializing_if = "Option::is_none"`).
- Estimate and totals: `rebuild-engine/w1-cost.md` (API and rules).

## Consumed interfaces

Confirm each with `rg` in the merged tree before use (the stage orchestrator does this first and
records differences with `loom memory decision`).

| Producer | Name | Status when written |
| --- | --- | --- |
| plan 1 | `loom::planning::runtime::RuntimePaths`, `planning::watch::Batch`, `planning::project::Project` | published |
| plan 2 | `loom::plan_server::context::PlanContext` with `on_batch(Box<dyn Fn(&PlanContext, &Batch) + Send + Sync>)`, `extensions().insert/get`, `authoring().holder()/is_holder()`, `read_only().problems`, `shutdown().register(ShutdownHook { name, drain, abort })`, `hub().broadcast(&PlanMessage)`, `runtime()`, `project()` | published |
| plan 2 | route module shape: `plan_server/<name>/mod.rs` with `pub fn routes() -> Vec<RouteSpec>` and `pub fn install(ctx: &Arc<PlanContext>)`, one line each in `plan_server/routes.rs::{all_routes, install_all}`; `RouteSpec { method, path, body, role, handler }`, `Role::{Viewer, Holder, HolderOrTake}`, `ApiRequest { ctx, head, browser, query, body }`, `ApiResponse::{json, error}` | published |
| plan 2 | wire: `PlanMessage` in `plan_server/wire/message.rs` (`#[serde(tag = "type", rename_all = "snake_case")]`), area structs in `plan_server/wire/<area>.rs`, fixtures `web/src/plan/api/fixtures/<name>.json`, Rust parity tests `plan_server/wire/tests_<area>.rs`, zod `web/src/plan/api/schema-<area>.ts` | published |
| plan 2 | `web/src/plan/header-extras.tsx` `<HeaderExtras/>`; `web/src/plan/api/socket.ts` `subscribe(type, handler)`, `web/src/plan/api/client.ts` `apiGet`, `apiPost`; atoms `isHolderAtom`, `canAuthorAtom` in `web/src/plan/state/atoms.ts` | published |
| plan 4 | `planning::summary::engine::EngineUpdate { evaluation, rewritten, deleted, invalidated }`, `evaluate::{Evaluation, SummaryState}`, `claims::{list_claims, clear_session_claims, release, Claim}`, `store::{expected_ids, source_of}` | published |
| plan 5 | `plan_server::concept::state::out_of_date_reason`, `ConceptPage.building: Option<{done,total}>`, `ConceptPage.estimate: Option<String>`, `web/src/plan/concept/page-header-extras.tsx` `<PageHeaderExtras page />`; plan 5 owns the `SummaryEngine` (called in its `on_batch` hook); `plan_server::concept::ConceptState` in `ctx.extensions()` with `subscribe(Box<dyn Fn(&PlanContext, &EngineUpdate) + Send + Sync>)` and `latest() -> Evaluation`; `ConceptPage` is built in `plan_server/concept/page.rs`; `PageContext` (Default) and `progress(&PageContext)` there; `concept/routes.rs::page_context(ctx)`; `state::out_of_date_reason(id, &SummaryState, &PageContext)`; `web/src/plan/concept/key-row-extras.tsx` `<KeyRowExtras page />` | confirmed by frontend |
| plan 6 | `plan_session::kind::SessionKind::Summary` | published |
| plan 7 | `plan_session::bridge::Bridge` (an `Arc<Bridge>` in `ctx.extensions()`): `start_session(StartRequest { kind: SessionKind, offer: Option<String> /* None for summary */, guidance: Box<dyn FnOnce(&SessionDir) -> Result<LaunchGuidance> + Send>, opening_origin: Option<Origin>, extra_env: Vec<(String, String)> }) -> Result<String, BridgeError>`, `end(id, EndMode::Teardown, EndCause::Caller) -> Result<Vec<String>, BridgeError>` (summary sessions have no queue; the returned Vec is dropped), `subscribe(Box<dyn Fn(&BridgeEvent)>)`, `BridgeEvent::{Started { id, kind }, State { id, state }, TurnStarted { id, turn_id }, TurnEnded { id, turn_id, changed }, Ended { id, kind, outcome: Closed|Marker|TornDown|Failed(String) }}`, `sessions()`; chat model `Turn.summaries_rebuilding: Vec<String>` (plan 10 fills it through a new `Bridge::set_rebuilding(turn: Option<(&str, &str)>, ids: Vec<String>)` in its own file `plan_session/bridge/rebuilding.rs`, publishing a `turn_updated` delta); the bridge tears a summary session down at its first main-agent Stop with no subagent running and denies its permission requests; `LOOM_PLAN_SESSION` = the session id | confirmed by sessions (plan 7's `doc/plans/briefs/plan-session-bridge/common.md`) |
| plan 7 | `BridgeEvent::SubagentStopped { id, agent_id }`, `Bridge::transcripts(&self, id) -> Option<Transcripts { main: Option<PathBuf>, subagents: Vec<(String, PathBuf)> }>`, recorded transcripts `loom/tests/fixtures/plan_session/transcripts/*.jsonl` (the cost contract copies one of these) | confirmed by sessions |
| plan 8 | `planning::guidance::{render_session_guidance(&SessionGuidanceKind::Summary { ids }, &Project, &RuntimePaths, &SessionDir, &GuidanceModels) -> Result<SessionGuidance>, GuidanceModels::load(root)}`, `impl From<SessionGuidance> for LaunchGuidance`; called inside plan 7's guidance closure with an owned `Project` snapshot and a clone of `ctx.runtime()` moved in (never re-resolved with `runtime_paths`: plan 8 reads its persisted config block from that directory) | confirmed by guidance-exec |
| plan 9 | `web/src/plan/chat/rebuilt-summary-extras.tsx` `<RebuiltSummaryExtras id turn />` (ends each rebuilt-summary row) | confirmed by frontend |
| plan 11 | consumes `Rebuilds::suspend(SuspendReason::PressureRun)` | answered |

## Wire (`plan_server/wire/summaries.rs`)

```text
SummaryPanel { running: Option<RunningRebuild { session, ids, done, total, started_at }>,
               queue: [id], failed: [{ id, error }], question: Option<SummaryQuestion>,
               last: Option<CostLine>, today: TokenVector, week: TokenVector,
               quota: QuotaSnapshot, read_only: Option<String>,
               terminal_claims: [id] /* claims made from a terminal, offered for release */,
               start_error: Option<String> /* last failed start, cleared on the next start */ }
SummaryQuestion { id, count, ids, estimate_tokens, basis: "last" | "per_byte" | "starting" }
SummaryTurnCost { session, turn, rows: [{ id, tokens: Option<u64> }] }
PlanMessage::{ SummaryPanel(SummaryPanel), SummaryQuestion(SummaryQuestion),
               SummaryQuestionClosed { id }, SummaryTurnCost(SummaryTurnCost) }
POST /api/summaries/rebuild  { "ids": [..] }                 -> 202 SummaryPanel
POST /api/summaries/answer   { "question": "<id>", "rebuild": bool } -> 200 SummaryPanel
POST /api/summaries/release  { "ids": [..] }                 -> 200 SummaryPanel
GET  /api/summaries/panel                                     -> 200 SummaryPanel
```

`QuotaSnapshot` is `quota::model::QuotaSnapshot` serialised as the dashboard already does (reuse
its serde; the planning fixture copies the dashboard's quota fixture shape).

## Rules for every worker

- Files at most 400 lines, functions at most 50 (`cargo test --test maintainability`); no ledger row.
- Never hold a scheduler lock across `start_summary`, `stop`, a file write, or a hub broadcast:
  compute events under the lock, act after releasing it.
- Every interpolated session label, summary id or error string printed to the terminal goes through
  `plan_server::lifecycle::safe`.
- `loom memory` for mistakes, decisions, surprises; never `loom knowledge`; never auto-memory; no
  state-changing git commands.
