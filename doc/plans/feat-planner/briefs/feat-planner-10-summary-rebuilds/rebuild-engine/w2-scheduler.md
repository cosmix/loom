# W2: scheduler, `Rebuilds`, the real spawner, attribution

Tier: opus, spawned with `effort: xhigh` (`loom-senior-software-engineer`): concurrent state across
the watcher thread, WebSocket handlers, session event callbacks and a tick thread. Read
`doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md` first; its "Scheduler rules" table is your
specification. You run beside W3 (routes and wire), after W1 (cost).

## Files you own

Under `loom/src/plan_server/summaries/`: `mod.rs`, `scheduler.rs`, `scheduler/tests.rs`,
`rebuilds.rs`, `spawner.rs`, `attribution.rs`, `attribution/tests.rs`; and `loom/src/plan_session/bridge/rebuilding.rs` plus one `mod` line in `loom/src/plan_session/bridge/mod.rs`.

## `scheduler.rs` (pure)

The API of the plan's CONTRACT SURFACE, exactly (the contracts are frozen against it). No I/O, no
threads, no clock reads: `now: Instant` is passed in; the spawner is a `&dyn SessionSpawner`.
Every rule of the table is one method or a branch of `try_start`. Keep the queue ordered (Button
ids first, then arrival order) and de-duplicated. `panel_state()` returns `SchedulerView`.
Stopping a session: `spawner.stop(session, release_claims)` then `Stopped { reason }` where
`StopReason::{ClaimsDone, Stalled, Ended, Failed(String), ServerStopping}`.

## `spawner.rs` (real)

`pub struct RealSpawner { ctx: Weak<PlanContext> }` implementing `SessionSpawner`:
`start_summary(ids)`: `bridge.start_session(StartRequest { kind: SessionKind::Summary, offer: None, guidance,
opening_origin: None, extra_env: vec![] })` (summary sessions have no chat) where `guidance` is a closure that loads
`GuidanceModels::load(root)` and returns
`render_session_guidance(&SessionGuidanceKind::Summary { ids }, &project, &runtime, dir, &models).map(Into::into)` with `runtime` a clone of `ctx.runtime()` moved into the closure (never re-resolve with `runtime_paths(root)`: it can disagree when `LOOM_PLAN_STATE` is set)
(plan 8's `From<SessionGuidance> for LaunchGuidance`). `stop(session, release)`:
`bridge.end(session, EndMode::Teardown, EndCause::Caller)` (plan 7: `-> Result<Vec<String>, BridgeError>`; a summary session queues nothing, so the Vec is dropped; an Err is logged with `plan_server::lifecycle::line` and the session is treated as still running until its `Ended` arrives), then `claims::clear_session_claims` when `release`.

## `plan_session/bridge/rebuilding.rs`

`impl Bridge { pub fn set_rebuilding(&self, turn: Option<(&str /* session */, &str /* turn */)>, ids: Vec<String>) }`:
set `Turn.summaries_rebuilding` on that turn and publish a `turn_updated` delta, the way the bridge's
own turn updates are published (read the merged `plan_session/bridge/` for the publishing helper).
Add `mod rebuilding;` to `plan_session/bridge/mod.rs` (one line).

## `attribution.rs`

`pub struct Attribution { paths: RuntimePaths, by_agent: HashMap<(String /* session */, String /* agent */), Vec<String /* ids */>> }`:
- `on_subagent_stopped(session, agent, transcript: &Path)` first scans the transcript's assistant
  `tool_use` blocks named `Bash`: an `input.command` whose whitespace-split words are
  `loom plan summary brief <ids...> [--note ...]` binds the ids (reject a command containing `;`,
  `&&`, `|`, backticks or `$(`: not a plain brief call).
- `on_subagent_stopped(session, agent)`: delete the claim files of the bound ids whose parsed
  `session` equals `session` (plan 4's `release` consults `LOOM_PLAN_SESSION` and
  `clear_session_claims` takes every id, so do the per-id delete here through
  `fs::safe_fs::safe_remove_in_workdir` on a dirfd of `paths.dir`); then compute the agent's
  transcript tokens (`cost::transcript_tokens`) and append `CostLine`s per common.md "Cost model".
- `last_seal_error(main_transcript: &Path) -> Option<String>`: the first line of the result text of the
  last `tool_result` whose `tool_use` was a Bash `loom plan summary seal <id>` call that failed
  (`is_error` true or output holding `: ` after a `drafts/` path); passed through
  `context::untrusted::inline_safe` and capped at 200 chars. The scheduler's stall rule
  (`tick` -> `Failed`) takes this text when `rebuilds.rs` supplies it.
- Naming rebuilt summaries on the authoring turn: when an authoring session's subagent briefs ids
  (seen on `SubagentStopped` or, for the open turn, on `TurnEnded` by scanning the subagents'
  transcripts), call `Bridge::set_rebuilding(Some((session, turn)), ids)` through a
  `trait RebuildingSink { fn set_rebuilding(&self, turn: Option<(&str, &str)>, ids: Vec<String>); }`
  implemented for `Bridge`, so tests use a recording sink.
- `on_session_ended(session)`: `claims::clear_session_claims(paths, session)`; for a summary
  session, append the batch `CostLine` for its own transcript.
- Return the per-turn rows (`SummaryTurnCost`) for authoring sessions so `rebuilds.rs` can broadcast
  them.

## `rebuilds.rs` (the facade in `ctx.extensions()`)

`pub struct Rebuilds { inner: Mutex<State> }` where `State { scheduler, attribution, last_update: Option<Arc<EngineUpdate>>, guards: HashMap<u64, SuspendReason> }`.
- Public API (common.md "Wire" and the plan's CONTRACT SURFACE): `suspend(reason) -> SuspendGuard`,
  `enqueue(ids, Trigger)`, `release_terminal_claims(ids)`, `panel() -> SummaryPanel`,
  `out_of_date_reason(id) -> Option<RebuildReason>`, `building() -> Option<Building>`,
  `estimate_text() -> Option<String>`, `on_update(&self, &EngineUpdate)`, plus `new(ctx)` and
  `new_for_test(spawner: Arc<dyn SessionSpawner>, read_only: Option<String>, broadcast: Arc<dyn Fn(&PlanMessage) + Send + Sync>)`
  (production passes a closure over `ctx.hub().broadcast`).
- `SuspendGuard` holds `Weak<Rebuilds>` and the token; `Drop` calls `resume`.
- Events from the scheduler are turned into hub broadcasts (`SummaryPanel`, `SummaryQuestion`,
  `SummaryQuestionClosed`, and plan 5's `concept_changed` naming the pages that show every id whose
  `out_of_date_reason` or `building` changed: `PlanMessage::ConceptChanged(ConceptChanged { pages, out_of_date, sources: vec![] })`
  from `plan_server::wire::concept`, `pages` = union of `plan_server::concept::page_keys_for_summary(id, &project)`;
  contract `rebuild-state-change-notifies-concept`) and terminal lines (`plan_server::lifecycle::line`, e.g.
  `summaries: rebuilding 4 (session <id>)`, `summaries: 9 out of date, asking`) AFTER the lock is
  released.

## `mod.rs`: `install(ctx)`

1. `Rebuilds::new(ctx)` into `ctx.extensions()`. Then plan 7's recovery callback (the bridge's
   `Bridge::on_recovered(callback)`, which runs at once when recovery is already done, since
   recovery's `Ended` events fire during plan 7's install before plan 10 subscribes. Signature:
   `Bridge::on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>)`,
   `RecoveryReport { closed: Vec<RecoveredSession>, torn_down: Vec<RecoveredSession>, left_alone: Vec<(id, socket, why)> }`,
   `RecoveredSession { id, kind, children }`. Live ids = `bridge.sessions()` at callback time;
   left-alone ids = `left_alone`'s ids; closed and torn-down sessions are dead, so their claims go): `attribution.sweep_after_recovery(live session ids, left-alone ids)` and the
   returned terminal claim ids into the scheduler's `terminal_claims` (contract
   `startup-sweeps-dead-claims`). Recovered sessions the bridge tears down also arrive as `Ended`.
2. `ctx.extensions().get::<plan_server::concept::ConceptState>()` then
   `ConceptState::subscribe(Box<dyn Fn(&PlanContext, &EngineUpdate) + Send + Sync>)`: plan 5 owns the
   `SummaryEngine` and calls it in its own `on_batch` hook; this callback receives every
   `EngineUpdate`. Call `scheduler.on_update(invalidated, out_of_date from update.evaluation)`,
   `seal_seen` for running ids whose state left Missing/Stale/Incomplete or whose file changed, and
   `claims_changed` from `claims::list_claims` filtered by the running session.
   `ConceptState::latest()` gives the latest `Evaluation` for `turn_ended`, `resume` and `enqueue`.
3. `bridge.subscribe(...)`: `TurnStarted`/`TurnEnded` of an authoring session -> `turn_started` /
   `turn_ended`; `State { Idle | Ended }` of an authoring session with a turn still open ->
   `authoring_state_idle` (Claude Code may write no Stop after an interrupt; contract
   `turn-suspension-released-on-idle`); a pressure session's `Started` and `Ended` -> its own `PressureRun` suspension
   (plan 11 may hold another; both are counted); `Ended` -> `session_ended` and
   `attribution.on_session_ended`; `SubagentStopped` -> `attribution.on_subagent_stopped` with the
   path from `bridge.transcripts(id)` (`Transcripts { main: Option<PathBuf>, subagents }`; `main` is
   None until the first SessionStart).
4. A tick thread (`std::thread::Builder::new().name("loom-plan-summaries")`, every 5 s until
   `ctx.is_stopping()`) calling `tick`, and, while a summary session runs, `claims_changed` from a
   fresh `claims::list_claims` (claims live in the runtime directory, which the watcher does not
   walk).
5. `quota::poller::spawn_quota_poller(ctx.runtime().dir.clone(), stop_flag)` (`quota/poller.rs:44`)
   with a stop flag set by a `ShutdownHook { name: "summaries", drain, abort }` that also drops the
   queue and open question.
6. Read-only: `SchedulerConfig { read_only: ctx.read_only().problems.first().map(|p| p.message.clone()), .. }`.

## Tests (yours)

`scheduler/tests.rs`: every row of the rules table, Button ordering, Current summaries leaving
`declined` and `failed`, stopping on server shutdown, `authoring_state_idle` with and without an open
turn, a late `TurnEnded` after it ignored, a `start_summary` Err (plan 8's `StartBlocks`) keeping the ids
queued with `start_error` set and retrying on the next batch, a `TurnEnded` while a `PressureRun`
guard lives queueing only. `attribution/tests.rs`: brief parsing (plain, with `--note`, rejected compound commands), per-summary vs batch lines, claim deletion limited to
the session, `last_seal_error` on a transcript with two seal calls (the second failed), and the recording
`RebuildingSink` receiving the turn's ids.

## Proof

`cargo test --lib plan_server::summaries::`, `cargo test --test plan_summary_rebuild_contracts`,
`cargo clippy --all-targets -- -D warnings`.
