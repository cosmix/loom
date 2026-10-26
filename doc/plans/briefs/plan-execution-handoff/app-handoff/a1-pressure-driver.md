# A1: the app's pressure driver

SEED: every file you own already exists as a one-line placeholder (the orchestrator created it so the crate compiles at every wave); replace it whole.

LANE: codex units write `options.rs` (CA1, the options and start validation) and `config_guard.rs` (CA2) in this stage's wave 2, in parallel with you; see `doc/plans/briefs/plan-execution-handoff/app-handoff/codex-units.md` for their pinned signatures. Call them; do not write those files.

Stage `app-handoff` of `doc/plans/PLAN-plan-execution-handoff.md`. Tier: opus
(`loom-senior-software-engineer`, effort xhigh: concurrent run thread, child processes,
shutdown and crash recovery). Never run git, never spawn subagents. One narrow check at the
end: `cargo test --lib plan_server::pressure::` from `loom/`. Frozen contract files
`tests/plan_handoff_contracts.rs` and `tests/plan_handoff_contracts/**` are READ-ONLY.

## Files you own (relative to `loom/`)

- `src/plan_server/pressure/routes.rs` (`pub fn all() -> Vec<RouteSpec>`) and `install.rs`
  (`pub fn run(ctx: &Arc<PlanContext>)`); codex unit CF6 owns `pressure/mod.rs`, which declares
  every file of this module and delegates to these two
- `src/plan_server/pressure/routes.rs`, `driver.rs`, `codex.rs`, `recover.rs`, `view.rs`,
  `teammates.rs`, `tests.rs`, `fixtures/` (new); `options.rs` and `config_guard.rs` are codex units CA1, CA2
- `src/plan_session/sandbox/mod.rs` (one line in `build_settings`: the top-level
  `teammateMode` key for the Pressure kind) and `src/plan_session/sandbox/tests_kinds.rs` (the
  pressure-kind snapshot gains the key; the stage declares this TI-edit)
- `tests/plan_handoff_operator.rs` (new; the operator-run test, last section)

Read-only: `src/plan_server/wire/pressure.rs` (F), `src/pressure/**` (stage
`pressure-pipeline`), plan 7's `src/plan_session/**`, plan 8's `src/planning/guidance/**`, plan
10's scheduler, plan 2's `src/plan_server/{context,routes,authoring,shutdown,lifecycle}.rs`.

## Spec

`doc/product/section_11.md` lines 21-148 (pressure testing from the app) and
`doc/product/section_08.md` lines 387-414 (what the panel shows), `doc/product/section_12.md`
lines 413-448 (pressure sessions). Every Claude step is a pressure session through plan 7; no
Claude runs headless; Codex runs as `codex exec` (principle 7).

## Interfaces you use (re-read each by symbol before writing; the stage description's step 1
records any difference)

- Pipeline (stage `pressure-pipeline`): `loom::pressure::{plan_steps, run_pipeline,
  PressureDriver, PressureResult, StepVerdict, PipelineInput, PipelineOutcome, PressureModels,
  resolve_plan_path, codex_report_path, completion_instruction, MarkerHome,
  codex_run::{spawn_codex_with_fallback, CodexRun, CodexOutcome, codex_skill_installed}}`.
- Plan 7 (`doc/plans/PLAN-plan-session-bridge.md`, stage `bridge-runtime`; full types in
  `doc/plans/briefs/plan-session-bridge/common.md`): `Arc<plan_session::bridge::Bridge>` from
  `ctx.extensions()`: `start_session(StartRequest { kind: SessionKind::Pressure { plan },
  guidance: Box<dyn FnOnce(&SessionDir) -> Result<LaunchGuidance> + Send>, opening_origin:
  Some(Origin::Offer), extra_env }) -> Result<String /* session id */, BridgeError>`,
  `subscribe(Box<dyn Fn(&BridgeEvent)>)` with `BridgeEvent::{Started, State, TurnStarted,
  TurnEnded, Ended { id, kind, outcome: Closed | Marker | TornDown | Failed(String) }}`,
  `interrupt(id) -> Vec<String>` (denies a pending permission first, section 07), `end(id,
  mode: EndMode::{FinalWrite, Teardown}, cause: EndCause::{Holder, GraceExpired, Draining,
  Caller}) -> Result<Vec<String>, BridgeError>` (returns the chat's queued texts), `record_child(id, "codex", ProcessIdentity)` (stored in
  `session.json`; plan 7's start-up recovery terminates verified children and tears a live
  pressure session down at once), `sessions()`, `chat_snapshot(for_holder)` (turns with
  `tokens` and `changed_files`), `release_session(id)` (on a Marker end plan 7 keeps verified
  children alive and the session directory in place until this call; Stop, Failed, abort and
  recovery still terminate children), `add_token_transcript(id, path: PathBuf) -> Result<(), BridgeError>` (the
  bridge's driver tails that file and routes each line through its own `Mapper::ingest_tokens_only`,
  so the turn's tokens and `TurnUpdated` deltas include it), and the
  `plan_server::chat::lease::AuthoringLeases` extension (`hold(&self) -> AuthoringLease`, an RAII guard): while a lease is held plan 7 does
  not release authoring when a chat session ends, and plan 7's grace-expiry handler leaves
  Pressure sessions to you. `SessionDir` (plan 6, `plan_session::kind`): `marker()` =
  `done/complete`, `codex_log()`; the directory of session `id` is
  `SessionDir::new(ctx.runtime().sessions.join(&id))`.
- Plan 6: the pressure settings allow exactly `Bash(touch <absolute marker path>)`; the
  completion instruction's `touch {}` with `dir.marker()` (absolute) matches it. YOU add the
  TOP-LEVEL settings key `"teammateMode": "in-process"` for the Pressure kind only, in plan 6's
  top-level assembler `src/plan_session/sandbox/mod.rs::build_settings` (not inside
  `permissions`, where Claude Code does not read it), update the pressure snapshot in
  `tests_kinds.rs`, and add a test that the Authoring and Summary settings lack it. Nobody has
  observed this key on this host: also, while each `/pressure` step runs, check every 5 s that
  its tmux server has exactly one pane (`tmux -L <socket> list-panes -a`, socket from the
  session record); a second pane ends the step `Failed` with reason "teammates opened panes:
  teammateMode not honoured by this Claude Code" (test it with a fake `tmux` script on PATH).
- Plan 8: `planning::guidance::{render_session_guidance, SessionGuidanceKind::Pressure {
  command, plan_rel, brief, model, effort }, PressureCommand, GuidanceModels}`; the guidance
  closure is `Box::new(move |dir| render_session_guidance(&kind_with(dir), &project, &runtime,
  dir, &models).map(|g| LaunchGuidance { opening_on_argv: true, ..LaunchGuidance::from(g) }))` (`runtime` a clone of
  `ctx.runtime()`; `opening_on_argv: true` is set HERE, for both pressure commands) where `brief = completion_instruction(&dir.marker(),
  MarkerHome::PlanSession)` is built inside the closure from `dir`.
- Plan 10: `ctx.extensions().get::<plan_server::summaries::Rebuilds>()`,
  `Rebuilds::suspend(SuspendReason::PressureRun) -> SuspendGuard`. While a guard lives no
  background summary session starts; on drop the scheduler recomputes the out-of-date set and
  queues the plan's summary itself (no `enqueue` call from you). Plan 10 also suspends on each
  pressure session's events, but the guard covers the gaps between steps (the Codex-only part of
  a round, the record write).
- Plan 2: `ctx.require_authoring(&BrowserId, Take::TakeIfFree)` (the published entry point),
  `ctx.authoring().{holder, subscribe, release}` with `AuthoringEvent::Freed { GraceExpired }`, `ctx.read_only()`,
  `ctx.hub().broadcast(&PlanMessage::PressureRun(..))`, `ctx.shutdown().register(ShutdownHook)`,
  `lifecycle::{line, safe}`, `RouteSpec`, `ApiRequest`, `ApiResponse`.

## Routes (`routes.rs`)

| Method, path | Role | Behaviour |
| --- | --- | --- |
| `GET /api/pressure/options` | Viewer | `PressureOptions`: `available` false with the first reason in this order: `CODEX_MISSING` (`crate::codex::find_codex_path()` errs), `CODEX_SKILL_MISSING` (`codex_skill_installed()` false), `READ_ONLY`, `NOT_HOLDER` (another browser holds authoring and it is not the requester), `RUNNING`. `plans` = written plans (plan 1 `PlanState::Written`) in section 01 order (`Project` reading order). `defaults` from `PressureModels::resolve_for(root, PressureModelFlags::default())`, rounds 2. Vocabularies: `crate::claude::CLAUDE_MODELS`, `crate::models::stage::ALLOWED_REASONING_EFFORTS`, `crate::codex::CODEX_MODELS`, `crate::codex::CODEX_EFFORTS`. |
| `POST /api/pressure/start` | HolderOrTake | Body `PressureStart`. Refusals (`ApiResponse::error(409 or 400, code, message)`): `codex_missing`, `codex_skill_missing`, `read_only`, `pressure_running`, `plan_not_written` (the path is not a written plan in the current `Project`, which also refuses `IN_PROGRESS-`, `DONE-`, `RETIRED-`, archive paths and anything outside `doc/plans/`), `invalid_model` (a slot outside its vocabulary), `invalid_rounds` (0 or > 10). On accept: 202 and the run starts on its own thread. |
| `POST /api/pressure/stop` | Holder | Stops the running run (below). 409 `no_run` when none. |
| `POST /api/pressure/confirm-config` | Holder | Clears plan 8's `StartBlocks` after a Codex configuration change (below). 409 `no_block` when none is set. |

## The run (`driver.rs`, `codex.rs`, `view.rs`)

1. Take authoring through `ctx.require_authoring(browser, Take::TakeIfFree)`, then at once hold
   an `AuthoringLeases::hold()` lease and take
   `Rebuilds::suspend(SuspendReason::PressureRun)`; both live until step 7 (px-state-17: the guard
   comes BEFORE the open chat's closing write, so the closing turn's `TurnEnded` starts no summary
   session). If an authoring session is live (`Bridge::sessions()` lists one of kind authoring),
   call `end(id, EndMode::FinalWrite, EndCause::Holder)` (the holder is present: its permission
   requests stay answerable), send the returned `Vec<String>` (the chat's queued messages) to the
   holder as `ChatFrame::QueueReturned { texts }` so they return to the composer, and wait for its `BridgeEvent::Ended` (section 11 "The
   dialog"; up to plan 7's 10-minute bound; broadcast a `PressureRunView` with no step started so
   the panel says the chat is closing). Authoring stays with the starter for the whole run: the
   lease stops plan 7 releasing it on that `Ended` and on every step's `Ended`.
   Grace expiry: subscribe to `ctx.authoring()`; on `AuthoringEvent::Freed { GraceExpired }`
   (the holder's tabs were gone 10 minutes) stop the run exactly as Stop does, with
   `ended.kind = Stopped` and `ended.by = "the holder was away for 10 minutes"` (decision of this
   plan: nobody is left to answer the run's prompts; proposed for section 11 in the fix log).
2. `AppDriver` implements `loom::pressure::PressureDriver`; the run thread calls
   `run_pipeline(&mut driver, &PipelineInput { rounds, invocation: plan_rel, report:
   codex_report_path(plan_fs), plan_path: plan_fs, claude_model: slots.claude_model, today:
   chrono::Local::now().date_naive() })`. No second step list: reuse the recording fake `PressureDriver` test from
   `src/pressure/tests.rs` to assert your driver goes only through `run_pipeline`.
3. `run_pressure(round, claude, codex)`: start the Claude session with
   `Bridge::start_session(StartRequest { kind: SessionKind::Pressure { plan }, guidance: the
   closure above with `PressureCommand::Pressure` and the `/pressure` slots, opening_origin:
   Some(Origin::Offer), extra_env: vec![(crate::claude::AGENT_TEAMS_ENV.into(), "1".into())] })`;
   then, with that session's `SessionDir`, take `config_guard::snapshot(root)` and start Codex with
   `spawn_codex_with_fallback(codex_path, root, codex, &dir.codex_log(), candidates, effort)`
   and call `record_child(id, "codex", ProcessIdentity { pid, start_time:
   process_start_time(pid) })` at once and again whenever `CodexRun::current_pid` changes (a
   fallback attempt; poll every 500 ms). Wait for the session through one `subscribe` callback
   installed at `install()` that forwards events for the run's session ids into an `mpsc`
   channel: `Ended { outcome: Marker }` → the Claude half is done; `Ended` with any other outcome
   before the marker → the step failed (`StepVerdict::Stop { step: "round <n> /pressure",
   reason }`, reason the `Failed` text or "the session ended before it finished"). Then wait for
   Codex
   (`is_finished`/`join`), classify its exit with `crate::claude::classify_exit` exactly as
   `commands/pressure/spawn.rs::should_stop` does (Abort stops; Warn continues with the log tail
   recorded on the step), and set the Codex status: `ReviewWritten` with `report_path` when the
   report file exists after a clean exit, `NoReview` otherwise, `Failed` with the last 2,000
   bytes of the log on an Abort. Only after Codex is joined and the log tail is read, call
   `release_session(id)` (px-seams-01: the marker end left Codex alive and `codex.log` in place).
   Then `config_guard::changed(&before, &config_guard::snapshot(root)?)`: a non-empty list ends
   the run (`StepVerdict::Stop { step: "round <n> Codex", reason: "Codex changed session
   configuration: <files>" }`) and calls plan 8's `planning::guidance::StartBlocks::block(ctx.runtime(), /* the server's RuntimePaths, the same render_session_guidance reads */
   <that reason>, &changed)`, which persists `<runtime dir>/config-block.json` and makes every
   later launch of ANY kind (a new pressure run included) refuse, across server restarts, until the
   holder confirms through `POST /api/pressure/confirm-config` (Holder; `StartBlocks::clear`; 409
   `no_block` when no file exists). `GET /api/pressure/options` reports a set block as
   `available: false` with its reason, and `POST /api/pressure/start` refuses `config_blocked`. `config_guard.rs`: `snapshot(root) -> ConfigSnapshot` hashes (sha256,
   no-follow bounded reads) every file under `.claude/` and the files `.mcp.json` and `CLAUDE.md`
   at the root, tracked or untracked; `changed(before, after) -> Vec<String>` lists added,
   removed and changed repo-relative paths, sorted (section 12 accepted limit: Codex may edit the
   tree, but configuration that runs code must not take effect unseen).
4. `run_address(round, slash)`: as the Claude half of 3 with `PressureCommand::Address` and
   the address slots; no Codex.
5. `delete_report`: `crate::claude::remove_if_exists`.
6. Every state change broadcasts `PlanMessage::PressureRun(view)`; `view.rs` builds
   `PressureRunView` from the driver's state: `step` = the running step's kind (`codex` once the
   Claude half of round n finished and Codex still runs), `tokens` per Claude step = the sum of
   that session's turns' `tokens` in `chat_snapshot` taken when it ends, `tokens_total` = their sum.
   Plan 7 counts subagent transcripts (`<session>/subagents/agent-*.jsonl`) but not in-process
   teammates: `teammates.rs` (yours) finds where the installed Claude Code writes in-process
   teammate transcripts for a session (start from `Bridge::transcripts(id)` and the session's
   transcript directory; confirm on the installed version, record the finding with `loom memory
   note`) and registers each with `Bridge::add_token_transcript(id, path)` as it appears (never a
   private Mapper: the usage must reach the turn's `tokens` the panel shows) (usage into the open turn, no chat item; section 11: teammates' messages are
   never rendered). Test it on a recorded teammate transcript fixture (`fixtures/teammate.jsonl`
   under the module's test dir) that its usage lands in the turn's tokens and adds no item.
   `edits` = the union of that session's turns' `changed_files` paths, `ended` from the
   `PipelineOutcome`: `Recorded { line }` → `Recorded`; `RecordSkipped` → `Failed` with reason
   `"plan moved during the run; not recorded: <line>"`; `Stopped` → `Failed { step, reason }` or
   `Stopped { by: <holder display name> }`.
7. After the run ends by any path: drop the suspension guard (plan 10 then queues the plan's
   summary), drop the lease, and release authoring with `ctx.authoring().release_if(&starter, Released("pressure run ended"))`,
   where `starter` is the `PublicId` recorded at `require` (from the start route's
   `ApiRequest.public_id`; `release_if` returns false and changes nothing when another browser holds) (a no-op if another browser took authoring
   after a grace expiry).

## Stop, failure, Ctrl-C, dead server

- Stop (`POST /api/pressure/stop`) and `Failed`: `interrupt(id)` (it denies a pending
  permission request first) then `end(id, EndMode::Teardown, EndCause::Caller)` for the running Claude step (or
  `release_session` when its marker already ended it); terminate Codex with
  `crate::process::identity::terminate_verified(identity)` (never a bare pid); delete the
  report; no record. Implement as a `stop: Arc<AtomicBool>` the driver checks between waits plus
  the direct teardown, so a Stop during either half returns `StepVerdict::Stop` promptly.
- Ctrl-C: register a `ShutdownHook { name: "pressure run", drain, abort }` at `install()`;
  `drain` performs Stop and waits for the run thread (it must finish quickly so the server's
  other drains, the authoring closing write, are not held by it); `abort` tears down and
  terminates without waiting.
- Dead server (`recover.rs`): plan 7's start-up recovery already tears a live pressure session
  down and terminates its verified `codex` child (missing evidence never destroys,
  `mistakes/tmux-backend.md`). At `install()`, register `Bridge::on_recovered(Box<dyn
  FnOnce(&RecoveryReport) + Send>)` (it runs when recovery ends, or at once if it already has);
  for every `RecoveredSession` in `report.torn_down` (and `closed`) whose `kind` is
  `SessionKind::Pressure { plan }`, delete
  `codex-<plan file>` beside that plan when present and print
  `lifecycle::line(&format!("pressure run on {} was interrupted when the last server stopped", safe(plan)))`.
  `recover.rs` also holds `pub fn codex_action(identity: Option<ProcessIdentity>) -> CodexAction`
  (`Terminate` when `verify_process_identity` is `VerifiedAlive`, `LeaveAlone` when
  `Unverifiable`, `Nothing` for `None` or `Dead`), which Stop and the Ctrl-C abort use before
  `terminate_verified`.

## Tests (`tests.rs`, yours)

With a stub bridge (use plan 7's test double if `common.md` names one; else wrap the calls you
make in a small `trait PressureSessions` inside `driver.rs`, the real impl delegating to
`Bridge` and a fake in tests) and a fake Codex binary (a shell script in a
`TempDir` that writes the report or exits 1): a full 2-round run records one line; a Stop in
round 1 writes no record and deletes the report; a Codex Abort stops the run and its step is
`Failed` with the log tail; a session `Failed` before the marker stops the run; the options
route's reason order; `plan_not_written` for an `IN_PROGRESS-` plan; the recorded-pressure
recovery deletes the report and prints the line.

Also, each its own test: every `disabled_reason` of the options route on its own (codex
missing via a `PATH` without codex, skill missing via `CODEX_HOME` at an empty dir, read-only,
another holder, a run already running) and the order when several hold; `pressure_running`,
`invalid_model`, `invalid_rounds` refusals of start; a live authoring session receives
`end(id, EndMode::FinalWrite, EndCause::Holder)`, its returned texts reach the holder as
`QueueReturned`, and its `Ended` is awaited before the first step's
`start_session`; the fake bridge receives `SessionKind::Pressure { plan }`, `extra_env`
containing `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, guidance whose brief contains the session's
absolute `done/complete` path, the `/pressure` slots for the pressure step and the `/address`
slots for the address step; `record_child(id, "codex", ..)` is called with the fake Codex's pid
and again after a fallback; the Codex status shows the model that ran after fallback
(`CodexOutcome::model_used`), not the requested one; a Stop terminates the fake Codex (its pid
no longer alive) and calls `interrupt` then `end(.., Teardown)`; the registered shutdown hook's
`drain` stops a running fake run and returns; the `SuspendGuard` is held from start until the
run ends and dropped after (observe through plan 10's `Rebuilds` test seam or a counter);
`teammateMode` is `in-process` in the Pressure settings and absent from the Authoring ones.

Every file under 400 lines, functions under 50.

## Added tests (fix round)

- After a live chat is ended and after each step's `Ended`, `ctx.authoring().holder()` is still
  the starter (fake bridge emits `Ended` for the chat and each step).
- `Freed { GraceExpired }` mid-run stops the run, ended `Stopped` by "the holder was away for 10
  minutes", and no record is written.
- A `TurnEnded` emitted during the chat's closing write (before the first step) starts no summary
  session: the `SuspendGuard` is already held (observe through plan 10's `Rebuilds` test seam).
- A marker end with a recorded live fake Codex leaves the Codex process alive and `codex.log`
  readable; `release_session` is called once, after the join.
- A fake Codex that writes `.claude/settings.local.json` ends the run with the configuration
  reason and writes `config-block.json`; a second `POST /api/pressure/start` is refused
  `config_blocked`; a restart (new `install` over the same runtime dir) still reports the block;
  `POST /api/pressure/confirm-config` removes the file.
- A fake `tmux` reporting two panes ends the `/pressure` step `Failed` with the teammate reason.
- A teammate transcript registered through `Bridge::add_token_transcript` adds its usage to
  `chat_snapshot`'s turn tokens and no chat item (fake bridge records the call; plan 7 tests the
  tailing).

- The fake bridge receives `LaunchGuidance.opening_on_argv == true` for both the `/pressure` and
  the `/address` session, with `opening_message` `/loom-plan:pressure <plan>` and
  `/loom-plan:address <plan>`.
- A grace expiry during the run, then browser B taking authoring, then the run's end: B still
  holds (`release_if` with the starter's id).
- Closing a chat with two queued messages before a run sends one `QueueReturned` with both texts
  to the holder.

## Operator test (`tests/plan_handoff_operator.rs`)

`#[test] #[ignore = "needs a logged-in Claude Code, codex with its pressure skill, tmux and network"]`
functions, each beginning with `if loom::process::sandbox_probe::skip_unless(<claude, codex,
tmux on PATH and codex_skill_installed()>, "<name>", "<what is missing>") { return; }` (so
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a missing prerequisite into a failure):
`operator_pressure_run_records_one_line` (asserts the transcript of the first `/pressure` session shows the command's expansion, not a literal user message; scratch git repo in a `TempDir` with one valid written
plan; starts the planning server in-process with `loom::plan_server::start(ServerOptions { root, host: "127.0.0.1".parse()?, port: Some(0), read_only: loom::plan_server::prereq::ReadOnly::from_problems(vec![]) })` (plan 2's public API; `LOOM_HOME` at a `TempDir` so the runtime directory is the test's own), which returns `RunningServer { local, token, ctx }`; HTTP requests first `GET /?token=<token>` with `Host: 127.0.0.1:<port>` (`port = local.port()`) to collect the two cookies from the 302 (`loom_plan_<port>`, `loom_plan_browser_<port>`), then send them with `Origin: http://<local>` on every request (the shape of plan 2's `bare_loom_plan_serves_the_planning_app` contract); `RunningServer::stop` at the end. Plan 2's `#[cfg(test)]` helpers are not reachable from an integration test target; `POST
/api/pressure/start` with 1 round; wait up to 60 minutes for `ended`; assert exactly one new line
under `## Pressure tests` and that every `/pressure` tmux server listed one pane while it ran),
`operator_stopped_run_writes_no_record` (start, wait for the first step's first `TurnStarted`,
`POST /api/pressure/stop`; assert no record line and no `codex-<plan file>` beside the plan), and
`operator_commit_and_start_run` (a doc/ change committed with `commit_doc_changes`, then `POST
/api/run/start` with backend tmux; assert `.loom/work` exists; then run `loom stop` and `loom
clean --all` with `LOOM_CLEAN_YES=1` in the scratch repo).
